extern crate clap;
extern crate time;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

use portus::{CongAlg, Config, Datapath, DatapathInfo, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct Reno<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    report_option: RenoConfigReport,
    sc: Option<Scope>,
    sock_id: u32,
    ss_thresh: u32,
    cwnd: u32,
    curr_cwnd_reduction: u32,
    last_cwnd_reduction: time::Timespec,
    init_cwnd: u32,
    rtt: u32,
}

pub const DEFAULT_SS_THRESH: u32 = 0x7fffffff;

#[derive(Debug, Clone)]
pub enum RenoConfigReport {
    Ack,
    Rtt,
    Interval(time::Duration),
}

#[derive(Clone)]
pub struct RenoConfig {
    pub ss_thresh: u32,
    pub init_cwnd: u32,
    pub report: RenoConfigReport,
}

impl Default for RenoConfig {
    fn default() -> Self {
        RenoConfig {
            ss_thresh: DEFAULT_SS_THRESH,
            init_cwnd: 0,
            report: RenoConfigReport::Rtt,
        }
    }
}

impl<T: Ipc> Reno<T> {
    fn send_pattern(&self) {
        match self.control_channel.send_pattern(
            self.sock_id,
            match self.report_option {
                RenoConfigReport::Ack => make_pattern!(
                    pattern::Event::SetCwndAbs(self.cwnd) => 
                    pattern::Event::WaitNs(100_000_000) 
                ),
                RenoConfigReport::Rtt => make_pattern!(
                    pattern::Event::SetCwndAbs(self.cwnd) => 
                    pattern::Event::WaitRtts(1.0) => 
                    pattern::Event::Report
                ),
                RenoConfigReport::Interval(dur) => make_pattern!(
                    pattern::Event::SetCwndAbs(self.cwnd) => 
                    pattern::Event::WaitNs(dur.num_nanoseconds().unwrap() as u32) => 
                    pattern::Event::Report
                ),
            }
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
            if let RenoConfigReport::Ack = self.report_option {
                "
                    (def (acked 0) (sacked 0) (loss 0) (timeout false) (rtt 0) (inflight 0))
                    (bind Flow.inflight Pkt.packets_in_flight)
                    (bind Flow.rtt Pkt.rtt_sample_us)
                    (bind Flow.acked (+ Flow.acked Pkt.bytes_acked))
                    (bind Flow.sacked (+ Flow.sacked Pkt.packets_misordered))
                    (bind Flow.loss Pkt.lost_pkts_sample)
                    (bind Flow.timeout Pkt.was_timeout)
                    (bind isUrgent true)
                ".as_bytes()
            } else {
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
                ".as_bytes()
            }
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

    fn additive_increase_with_slow_start(&mut self, acked: u32) {
        let mut new_bytes_acked = acked;
        if self.cwnd < self.ss_thresh {
            // increase cwnd by 1 per packet, until ssthresh
            if self.cwnd + new_bytes_acked > self.ss_thresh {
                new_bytes_acked -= self.ss_thresh - self.cwnd;
                self.cwnd = self.ss_thresh;
            } else {
                self.cwnd += new_bytes_acked;
                new_bytes_acked = 0;
            }
        }

        // increase cwnd by 1 / cwnd per packet
        self.cwnd += (1448. * (new_bytes_acked as f64 / self.cwnd as f64)) as u32;
    }

    fn handle_timeout(&mut self) {
        self.ss_thresh /= 2;
        if self.ss_thresh < self.init_cwnd {
            self.ss_thresh = self.init_cwnd;
        }

        self.cwnd = self.init_cwnd;
        self.curr_cwnd_reduction = 0;

        self.logger.as_ref().map(|log| {
            warn!(log, "timeout"; 
                "curr_cwnd (pkts)" => self.cwnd / 1448, 
                "ssthresh" => self.ss_thresh,
            );
        });

        self.send_pattern();
        return;
    }

    /// Handle sacked or lost packets
    /// Only call with loss > 0 || sacked > 0
    fn cwnd_reduction(&mut self, loss: u32, sacked: u32, acked: u32) {
        if time::now().to_timespec() - self.last_cwnd_reduction > time::Duration::microseconds((self.rtt as f64 * 2.0) as i64) {
            self.curr_cwnd_reduction = 0;
        }

        // if loss indicator is nonzero
        // AND the losses in the lossy cwnd have not yet been accounted for
        // OR there is a partial ACK AND cwnd was probing ss_thresh
        if loss > 0 && self.curr_cwnd_reduction == 0 || (acked > 0 && self.cwnd == self.ss_thresh) {
            self.cwnd /= 2;
            self.last_cwnd_reduction = time::now().to_timespec();
            if self.cwnd <= self.init_cwnd {
                self.cwnd = self.init_cwnd;
            }

            self.ss_thresh = self.cwnd;
            self.send_pattern();
        }

        self.curr_cwnd_reduction += sacked + loss;
        self.logger.as_ref().map(|log| {
            info!(log, "loss"; 
                "curr_cwnd (pkts)" => self.cwnd / 1448, 
                "loss" => loss, 
                "sacked" => sacked, 
                "curr_cwnd_deficit" => self.curr_cwnd_reduction,
                "since_last_drop" => ?(time::now().to_timespec() - self.last_cwnd_reduction),
            );
        });
    }
}

impl<T: Ipc> CongAlg<T> for Reno<T> {
    type Config = RenoConfig;

    fn name() -> String {
        String::from("reno")
    }

    fn create(control: Datapath<T>, cfg: Config<T, Reno<T>>, info: DatapathInfo) -> Self {
        let mut s = Self {
            control_channel: control,
            logger: cfg.logger,
            report_option: cfg.config.report,
            cwnd: info.init_cwnd,
            init_cwnd: info.init_cwnd,
            curr_cwnd_reduction: 0,
            last_cwnd_reduction: time::now().to_timespec() - time::Duration::milliseconds(500),
            sc: None,
            sock_id: info.sock_id,
            ss_thresh: cfg.config.ss_thresh,
            rtt: 0,
        };

        if cfg.config.init_cwnd != 0 {
            s.cwnd = cfg.config.init_cwnd;
            s.init_cwnd = cfg.config.init_cwnd;
        }

        s.logger.as_ref().map(|log| {
            debug!(log, "starting reno flow"; "sock_id" => info.sock_id);
        });

        s.sc = s.install_fold();
        s.send_pattern();
        s
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
        let (acked, was_timeout, sacked, loss, rtt, inflight) = self.get_fields(m);
        self.rtt = rtt;
        if was_timeout {
            self.handle_timeout();
            return;
        }

        // increase the cwnd corresponding to new in-order cumulative ACKs
        self.additive_increase_with_slow_start(acked);

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
                "curr_cwnd (pkts)" => self.cwnd / 1448u32, 
                "inflight (pkts)" => inflight, 
                "loss" => loss, 
                "ssthresh" => self.ss_thresh,
                "rtt" => rtt,
            );
        });
    }
}
