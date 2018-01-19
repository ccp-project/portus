extern crate clap;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

extern crate time;

use portus::{CongAlg, Config, Datapath, DatapathInfo, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct Cubic<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    curr_cwnd_reduction: u32,
    pkt_size: u32,
    init_cwnd: f64,
    cwnd: f64,
    
//state for cubic
    ss_thresh: f64,
    cwnd_cnt: f64,
    tcp_friendliness: bool,
    beta: f64,
    fast_convergence: bool,
    c: f64,
    wlast_max: f64,
    epoch_start: f64,
    origin_point: f64,
    d_min:  f64,
    wtcp: f64,
    k: f64,
    ack_cnt: f64,
    cnt: f64,
}

pub const DEFAULT_SS_THRESH: f64 = 0x7fffffff as f64;

#[derive(Clone)]
pub struct CubicConfig {
    pub init_cwnd: f64,
    pub ss_thresh: f64,
}

impl Default for CubicConfig {
    fn default() -> Self {
        CubicConfig {
            init_cwnd: 10f64,
            ss_thresh: DEFAULT_SS_THRESH,
        }
    }
}

impl<T: Ipc> Cubic<T> {
    fn send_pattern(&self) {
        match self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::SetCwndAbs((self.cwnd * (self.pkt_size as f64)) as u32) => 
                pattern::Event::WaitRtts(0.5) => 
                pattern::Event::Report
            ),
        ) {
            Ok(_) => (),
            Err(e) => {
                self.logger.as_ref().map(|log| {
                    warn!(log, "send_pattern"; "err" => ?e);
                });
            }
        }
    }

    fn install_fold(&self) -> Option<Scope> {
        match self.control_channel.install_measurement(
            self.sock_id,
            "
                (def (acked 0) (sacked 0) (loss 0) (timeout false) (rtt 0) (inflight 0))
                (bind Flow.inflight Pkt.packets_in_flight)
                (bind Flow.rtt Pkt.rtt_sample_us)
                (bind Flow.acked (+ Flow.acked Pkt.bytes_acked))
                (bind Flow.sacked (+ Flow.sacked Pkt.packets_misordered))
                (bind Flow.loss Pkt.lost_pkts_sample)
                (bind Flow.timeout Pkt.was_timeout)
                (bind isUrgent Pkt.was_timeout)
                (bind isUrgent (!if isUrgent (> Flow.loss 0)))
            "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    fn get_fields(&mut self, m: Measurement) -> (u32, bool, u32, u32, u32, u32) {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let ack = m.get_field(&String::from("Flow.acked"), sc).expect(
            "expected acked field in returned measurement",
        ) as u32;

        let sack = m.get_field(&String::from("Flow.sacked"), sc).expect(
            "expected sacked field in returned measurement",
        ) as u32;

        let was_timeout = m.get_field(&String::from("Flow.timeout"), sc).expect(
            "expected timeout field in returned measurement",
        ) as u32;

        let inflight = m.get_field(&String::from("Flow.inflight"), sc).expect(
            "expected inflight field in returned measurement",
        ) as u32;

        let loss = m.get_field(&String::from("Flow.loss"), sc).expect(
            "expected loss field in returned measurement",
        ) as u32;

        let rtt = m.get_field(&String::from("Flow.rtt"), sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;

        (ack, was_timeout == 1, sack, loss, rtt, inflight)
    }

    fn cubic_increase_with_slow_start(&mut self, acked: u32, rtt: u32) {
        let new_bytes_acked = acked;
        let f_rtt = (rtt as f64)*0.000001;
        let mut no_of_acks = ((new_bytes_acked as f64)/(self.pkt_size as f64)) as u32;
        if self.cwnd <= self.ss_thresh {
            if self.cwnd+(no_of_acks as f64)< self.ss_thresh {
                self.cwnd += no_of_acks as f64;
                no_of_acks = 0;
            } else {
                no_of_acks -= (self.ss_thresh - self.cwnd) as u32;
                self.cwnd = self.ss_thresh;
            }
        }
        for _i in 0..no_of_acks{
            if self.d_min <= 0.0 || f_rtt < self.d_min {
                self.d_min = f_rtt;
            }

            self.cubic_update();
            if self.cwnd_cnt > self.cnt {
                self.cwnd = self.cwnd + 1.0;
                self.cwnd_cnt = 0.0;
            } else {
                self.cwnd_cnt = self.cwnd_cnt + 1.0;
            }
        }
        
    }

    fn cubic_update(&mut self){
        self.ack_cnt = self.ack_cnt + 1.0;
        if self.epoch_start <= 0.0 {
            self.epoch_start = (time::get_time().sec as f64) + (time::get_time().nsec as f64)/1000000000.0;
            if self.cwnd < self.wlast_max {
                let temp = (self.wlast_max-self.cwnd)/self.c;
                self.k = (temp.max(0.0)).powf(1.0/3.0);
                self.origin_point = self.wlast_max;
            } else {
                self.k = 0.0;
                self.origin_point = self.cwnd;
            }

            self.ack_cnt = 1.0;
            self.wtcp = self.cwnd
        }

        let t = (time::get_time().sec as f64) + (time::get_time().nsec as f64)/1000000000.0 + self.d_min - self.epoch_start;
        let target = self.origin_point + self.c*((t-self.k)*(t-self.k)*(t-self.k));
        if target > self.cwnd {
            self.cnt = self.cwnd / (target - self.cwnd);
        } else {
            self.cnt = 100.0 * self.cwnd;
        }

        if self.tcp_friendliness {
            self.cubic_tcp_friendliness();
        }
    }

    fn cubic_tcp_friendliness(&mut self) {
        self.wtcp = self.wtcp + (((3.0 * self.beta) / (2.0 - self.beta)) * (self.ack_cnt / self.cwnd));
        self.ack_cnt = 0.0;
        if self.wtcp > self.cwnd {
            let max_cnt = self.cwnd / (self.wtcp - self.cwnd);
            if self.cnt > max_cnt {
                self.cnt = max_cnt;
            }
        }
    }

    fn cubic_reset(&mut self) {
        self.wlast_max = 0.0;
        self.epoch_start = -0.1;
        self.origin_point = 0.0;
        self.d_min = -0.1;
        self.wtcp = 0.0;
        self.k = 0.0;
        self.ack_cnt = 0.0;
    }

    fn handle_timeout(&mut self) {
        self.ss_thresh /= 2.0;
        if self.ss_thresh < self.init_cwnd {
            self.ss_thresh = self.init_cwnd;
        }

        self.cwnd = self.init_cwnd;
        self.curr_cwnd_reduction = 0;

        self.cubic_reset();

        self.logger.as_ref().map(|log| {
            warn!(log, "timeout"; 
                "curr_cwnd (pkts)" => self.cwnd, 
                "ssthresh" => self.ss_thresh,
            );
        });

        self.send_pattern();
        return;
    }

    /// Handle sacked or lost packets
    /// Only call with loss > 0 || sacked > 0
    fn cwnd_reduction(&mut self, loss: u32, sacked: u32, acked: u32) {
        // if loss indicator is nonzero
        // AND the losses in the lossy cwnd have not yet been accounted for
        // OR there is a partial ACK AND cwnd was probing ss_thresh
        if loss > 0 && self.curr_cwnd_reduction == 0 || (acked > 0 && self.cwnd == self.ss_thresh) {
            self.epoch_start = -0.1;
            if self.cwnd < self.wlast_max && self.fast_convergence {
                self.wlast_max = self.cwnd * ((2.0 - self.beta) / 2.0);
            } else {
                self.wlast_max = self.cwnd;
            }

            self.cwnd = self.cwnd * (1.0 - self.beta);
            if self.cwnd <= self.init_cwnd {
                self.cwnd = self.init_cwnd;
            }

            self.ss_thresh = self.cwnd;
            self.send_pattern();
        }

        self.curr_cwnd_reduction += sacked + loss;
        self.logger.as_ref().map(|log| {
            info!(log, "loss"; "curr_cwnd (pkts)" => self.cwnd, "loss" => loss, "sacked" => sacked, "curr_cwnd_deficit" => self.curr_cwnd_reduction);
        });
    }
}

impl<T: Ipc> CongAlg<T> for Cubic<T> {
    type Config = CubicConfig;

    fn name() -> String {
        String::from("cubic")
    }

    fn create(control: Datapath<T>, cfg: Config<T, Cubic<T>>, info: DatapathInfo) -> Self {
        let mut s = Self {
            control_channel: control,
            logger: cfg.logger,
            curr_cwnd_reduction: 0,
            sc: None,
            sock_id: info.sock_id,
            init_cwnd: cfg.config.init_cwnd/1500.0,
            ss_thresh: cfg.config.ss_thresh/1500.0,
            pkt_size: 1500u32,
            cwnd: cfg.config.init_cwnd/1500.0,
            cwnd_cnt: 0.0f64,
            tcp_friendliness: true,
            beta: 0.3f64,
            fast_convergence: true,
            c: 0.4f64,
            wlast_max: 0.0f64,
            epoch_start: -0.1f64,
            origin_point: 0.0f64,
            d_min:  -0.1f64,
            wtcp: 0.0f64,
            k: 0.0f64,
            ack_cnt: 0.0f64,
            cnt: 0.0f64,
        };


        s.logger.as_ref().map(|log| {
            debug!(log, "starting reno flow"; "sock_id" => info.sock_id);
        });

        s.sc = s.install_fold();
        s.send_pattern();
        s
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
        let (acked, was_timeout, sacked, loss, rtt, inflight) = self.get_fields(m);
        if was_timeout {
            self.handle_timeout();
            return;
        }

        // increase the cwnd corresponding to new in-order cumulative ACKs
        self.cubic_increase_with_slow_start(acked,rtt);

        if loss > 0 || sacked > 0 {
            self.cwnd_reduction(loss, sacked, acked);
        } else if acked < self.curr_cwnd_reduction {
            self.curr_cwnd_reduction -= acked / 1448u32;
        } else {
            self.curr_cwnd_reduction = 0;
        }

        if self.curr_cwnd_reduction > 0 {
            self.logger.as_ref().map(|log| {
                debug!(log, "in cwnd reduction"; "acked" => acked / 1448u32, "deficit" => self.curr_cwnd_reduction);
            });
            return;
        }

        self.send_pattern();

        self.logger.as_ref().map(|log| {
            debug!(log, "got ack"; 
                "acked(pkts)" => acked / 1448u32, 
                "curr_cwnd (pkts)" => self.cwnd, 
                "inflight (pkts)" => inflight, 
                "loss" => loss, 
                "ssthresh" => self.ss_thresh,
                "rtt" => rtt,
            );
        });
    }
}
