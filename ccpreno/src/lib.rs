#[macro_use]
extern crate slog;

#[macro_use]
extern crate portus;

extern crate time;

use time::Timespec;
use portus::{CongAlg, Measurement};
use portus::pattern;
use portus::ipc::{Ipc, Backend};
use portus::lang::Scope;

pub struct Reno<T: Ipc> {
    control_channel: Option<Backend<T>>,
    sc: Option<Scope>,
    sock_id: u32,
    ss_thresh: u32,
    cwnd: u32,
    last_ack: u32,
    last_cwnd_reduction: Timespec,
    init_cwnd: u32,
}

impl<T: Ipc> Default for Reno<T> {
    fn default() -> Self {
        Reno {
            control_channel: None,
            sc: None,
            sock_id: Default::default(),
            ss_thresh: Default::default(),
            cwnd: Default::default(),
            last_ack: Default::default(),
            last_cwnd_reduction: Timespec::new(0, 0),
            init_cwnd: Default::default(),
        }
    }
}

impl<T: Ipc> Reno<T> {
    fn send_pattern(&self) {
        let _ = self.control_channel.as_ref().map(|ch| {
            ch.send_pattern(
                self.sock_id,
                make_pattern!(
                    pattern::Event::SetCwndAbs(self.cwnd) => 
                    pattern::Event::WaitNs(1000) => 
                    pattern::Event::Report
                ),
            )
        });
    }

    fn install_fold(&mut self) {
        let ch = self.control_channel.as_ref().expect(
            "channel should be initialized",
        );

        if let Ok(scope) = ch.install_measurement(
            self.sock_id,
            "
                (def (ack 0) (loss 0) (rtt 0))
                (bind Flow.rtt Rtt)
                (bind Flow.ack (wrapped_max Flow.ack Ack))
                (bind Flow.loss Loss)
                (bind isUrgent (> Flow.loss 0))
            "
                .as_bytes(),
        )
        {
            self.sc = Some(scope);
        }
    }

    fn get_fields(&mut self, m: Measurement) -> (u32, u32, u32, u32) {
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

        (ack, was_urgent, loss, rtt)
    }

    fn handle_urgent(&mut self, log_opt: Option<slog::Logger>, loss: u32, rtt_us: u32) {
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

        log_opt.as_ref().map(|log| {
            debug!(log, "urgent"; "curr_cwnd (B)" => self.cwnd, "loss" => loss);
        });

        self.send_pattern();
    }
}

impl<T: Ipc> CongAlg<T> for Reno<T> {
    fn name(&self) -> String {
        String::from("reno")
    }

    fn create(
        &mut self,
        control: Backend<T>,
        log_opt: Option<slog::Logger>,
        sock_id: u32,
        start_seq: u32,
        init_cwnd: u32,
    ) {
        self.control_channel = Some(control);
        self.sock_id = sock_id;
        self.last_ack = start_seq;
        self.cwnd = init_cwnd;
        self.ss_thresh = 0x7fffff;
        self.init_cwnd = init_cwnd;

        log_opt.as_ref().map(|log| {
            debug!(log, "starting reno flow"; "sock_id" => sock_id, "start_seq" => start_seq);
        });

        self.install_fold();
        self.send_pattern();
    }

    fn measurement(&mut self, log_opt: Option<slog::Logger>, _sock_id: u32, m: Measurement) {
        let (ack, was_urgent, loss, rtt) = self.get_fields(m);
        if was_urgent != 0 {
            self.handle_urgent(log_opt, loss, rtt);
            return;
        }

        // Handle integer overflow / sequence wraparound
        let mut new_bytes_acked = if ack < self.last_ack {
            (u32::max_value() - self.last_ack) + ack
        } else {
            ack - self.last_ack
        };

        self.last_ack = ack;
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

        log_opt.as_ref().map(|log| {
            debug!(log, "got ack"; "seq" => ack, "curr_cwnd (B)" => self.cwnd, "loss" => loss, "ssthresh" => self.ss_thresh);
        });
    }
}
