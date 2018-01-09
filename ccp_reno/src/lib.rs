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

pub struct Reno<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    ss_thresh: u32,
    cwnd: u32,
    last_cwnd_reduction: Timespec,
    init_cwnd: u32,
}

impl<T: Ipc> Reno<T> {
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
                (def (acked 0) (loss 0) (timeout false) (rtt 0) (inflight 0))
                (bind Flow.inflight Pkt.packets_in_flight)
                (bind Flow.rtt Pkt.rtt_sample_us)
                (bind Flow.acked (+ Flow.acked Pkt.bytes_acked))
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

    fn get_fields(&mut self, m: Measurement) -> (u32, bool, bool, u32, u32, u32) {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let ack = m.get_field(&String::from("Flow.acked"), sc).expect(
            "expected ack field in returned measurement",
        ) as u32;

        let was_urgent = m.get_field(&String::from("isUrgent"), sc).expect(
            "expected isUrgent field in returned measurement",
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

        (ack, was_urgent == 1, was_timeout == 1, loss, rtt, inflight)
    }

    fn handle_urgent(&mut self, was_timeout: bool, loss: u32, rtt_us: u32) {
        if time::get_time() - self.last_cwnd_reduction <
            time::Duration::nanoseconds((rtt_us as u64 * 1000) as i64)
        {
            return;
        }

        self.last_cwnd_reduction = time::get_time();

        if was_timeout {
            self.ss_thresh /= 2;
            if self.ss_thresh < self.init_cwnd {
                self.ss_thresh = self.init_cwnd;
            }

            self.cwnd = self.init_cwnd;

            self.logger.as_ref().map(|log| {
                warn!(log, "timeout"; 
                    "curr_cwnd (pkts)" => self.cwnd / 1460, 
                    "ssthresh" => self.ss_thresh,
                    "loss" => loss,
                );
            });
        } else if loss > 0 {
            // else, treat as isolated loss
            self.cwnd /= 2;
            if self.cwnd <= self.init_cwnd {
                self.cwnd = self.init_cwnd;
            }

            self.ss_thresh = self.cwnd;

            self.logger.as_ref().map(|log| {
                info!(log, "urgent"; "curr_cwnd (pkts)" => self.cwnd / 1460, "loss" => loss);
            });
        }

        self.send_pattern();
    }
}

impl<T: Ipc> CongAlg<T> for Reno<T> {
    fn name(&self) -> String {
        String::from("reno")
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
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting reno flow"; "sock_id" => sock_id);
        });

        s.sc = s.install_fold();
        s.send_pattern();
        s
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
        let (acked, was_urgent, was_timeout, loss, rtt, inflight) = self.get_fields(m);
        if was_urgent {
            self.handle_urgent(was_timeout, loss, rtt);
            return;
        }

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
        self.cwnd += 1460u32 * (new_bytes_acked / self.cwnd);
        self.send_pattern();

        self.logger.as_ref().map(|log| {
            debug!(log, "got ack"; 
                "acked(bytes)" => acked, 
                "curr_cwnd (pkts)" => self.cwnd / 1460, 
                "inflight (pkts)" => inflight, 
                "loss" => loss, 
                "ssthresh" => self.ss_thresh
            );
        });
    }
}
