#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;
extern crate time;

use time::Timespec;
use portus::{CongAlg, Datapath, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct Dctcp<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    ss_thresh: u32,
    cwnd: u32,
    last_cwnd_reduction: Timespec,
    init_cwnd: u32,
    alpha: f32,
}

impl<T: Ipc> Dctcp<T> {
    fn send_pattern(&self) {
        match self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::SetCwndAbs(self.cwnd) => 
                pattern::Event::WaitRtts(1.0) => 
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
                (def (ack 0) (loss 0) (rtt 0) (ecns 0))
                (bind Flow.ecns (+ Flow.ecns Pkt.ecn_bytes))
                (bind Flow.rtt Pkt.rtt_sample_us)
                (bind Flow.acked (+ Flow.acked Pkt.bytes_acked))
                (bind Flow.loss Pkt.lost_pkts_sample)
                (bind isUrgent (> Flow.loss 0))
            "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    fn get_fields(&mut self, m: Measurement) -> (u32, u32, u32, u32, u32) {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let ack = m.get_field(&String::from("Flow.ack"), sc).expect(
            "expected ack field in returned measurement",
        ) as u32;

        let was_urgent = m.get_field(&String::from("isUrgent"), sc).expect(
            "expected isUrgent field in returned measurement",
        ) as u32;

        let loss = m.get_field(&String::from("Flow.loss"), sc).expect(
            "expected loss field in returned measurement",
        ) as u32;

        let rtt = m.get_field(&String::from("Flow.rtt"), sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;

        let ecn = m.get_field(&String::from("Flow.ecns"), sc).expect(
            "expected ecns field in returned measurement",
        ) as u32;

        (ack, was_urgent, loss, rtt, ecn)
    }

    fn handle_urgent(&mut self, loss: u32, rtt_us: u32) {
        if time::get_time() - self.last_cwnd_reduction <
            time::Duration::nanoseconds((rtt_us as u64 * 1000) as i64)
        {
            return;
        }

        self.last_cwnd_reduction = time::get_time();

        if loss > self.cwnd / 2 {
            // treat as timeout if at least half the cwnd was lost
            self.ss_thresh /= 2;
            self.cwnd = self.init_cwnd;
        } else if loss > 0 {
            // else, treat as isolated loss
            self.cwnd /= 2;
            if self.cwnd <= self.init_cwnd {
                self.cwnd = self.init_cwnd;
            }

            self.ss_thresh = self.cwnd;
        }

        self.logger.as_ref().map(|log| {
            debug!(log, "urgent"; "curr_cwnd (B)" => self.cwnd, "loss" => loss);
        });

        self.send_pattern();
    }
}

impl<T: Ipc> CongAlg<T> for Dctcp<T> {
    fn name() -> String {
        String::from("dctcp")
    }

    fn create(
        control: Datapath<T>,
        log_opt: Option<slog::Logger>,
        sock_id: u32,
        init_cwnd: u32,
    ) -> Self {
        let mut s = Self {
            control_channel: control,
            logger: log_opt,
            cwnd: init_cwnd,
            init_cwnd: init_cwnd,
            last_cwnd_reduction: time::get_time(),
            sc: None,
            sock_id: sock_id,
            ss_thresh: 0x7fffffff,
            alpha: 0.0,
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting dctcp flow"; "sock_id" => sock_id);
        });

        s.sc = s.install_fold();
        s.send_pattern();
        s
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
        let (mut new_bytes_acked, was_urgent, loss, rtt, ecns) = self.get_fields(m);
        if was_urgent != 0 {
            self.handle_urgent(loss, rtt);
            return;
        }

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

        // update DCTCP alpha
        // alpha <- (1 - g) * alpha + g * F
        // where F is the fraction of ECN-marked packets in the last window
        self.alpha = 0.2 * self.alpha + 0.8 * (ecns as f32 / self.cwnd as f32);
        if ecns > 0 {
            self.cwnd = (self.cwnd as f32 * (1.0 - self.alpha / 2.0)) as u32;
        } else {
            // increase cwnd by 1 / cwnd per packet
            self.cwnd += 1460u32 * (new_bytes_acked / self.cwnd);
        }

        self.send_pattern();

        self.logger.as_ref().map(|log| {
            debug!(log, "got ack"; "curr_cwnd (B)" => self.cwnd, "loss" => loss, "ssthresh" => self.ss_thresh);
        });
    }
}
