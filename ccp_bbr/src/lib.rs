#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

use portus::{CongAlg, Datapath, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct Bbr<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    bottle_rate: u32,
    rtt_us: u32,
}

impl<T: Ipc> Bbr<T> {
    fn send_pattern(&self) {
        self.logger.as_ref().map(|log| {
            debug!(log, "setting pattern"; 
                "cwnd, pkts" => (self.bottle_rate as f32 * 1.25 * self.rtt_us as f32 / 1e6) as u32 / 1460,
                "set rate, Mbps" => (self.bottle_rate as f32 * 0.95) as u32 / 125000,
                "up pulse rate, Mbps" => (self.bottle_rate as f32 * 1.25 * 0.95) as u32 / 125000,
                "down pulse rate, Mbps" => (self.bottle_rate as f32 * 0.75 * 0.95) as u32 / 125000,
            );
        });

        match self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::SetRateAbs((self.bottle_rate as f32 * 1.25 * 0.95) as u32) => 
                pattern::Event::SetCwndAbs((self.bottle_rate as f32 * 1.25 * 0.95 * self.rtt_us as f32 / 1e6) as u32) => 
                pattern::Event::WaitRtts(1.0) => 
                pattern::Event::Report =>
                pattern::Event::SetRateAbs((self.bottle_rate as f32 * 0.75 * 0.95) as u32) => 
                pattern::Event::WaitRtts(1.0) => 
                pattern::Event::Report =>
                pattern::Event::SetRateAbs((self.bottle_rate as f32 * 0.95) as u32) => 
                pattern::Event::WaitRtts(6.0) => 
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
                (def (loss 0) (rtt 0) (rin 0) (rout 0))
                (bind Flow.loss (+ Flow.loss Pkt.lost_pkts_sample))
                (bind Flow.rtt (ewma 3 Pkt.rtt_sample_us))
                (bind Flow.rin Pkt.rate_outgoing)
                (bind Flow.rout (max Flow.rout Pkt.rate_incoming))
            "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    fn get_fields(&mut self, m: Measurement) -> (u32, u32, u32, u32) {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let rtt = m.get_field(&String::from("Flow.rtt"), sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;

        let loss = m.get_field(&String::from("Flow.loss"), sc).expect(
            "expected loss field in returned measurement",
        ) as u32;

        let rin = m.get_field(&String::from("Flow.rin"), sc).expect(
            "expected rin field in returned measurement",
        ) as u32;

        let rout = m.get_field(&String::from("Flow.rout"), sc).expect(
            "expected rout field in returned measurement",
        ) as u32;

        (loss, rtt, rin, rout)
    }
}

impl<T: Ipc> CongAlg<T> for Bbr<T> {
    fn name(&self) -> String {
        String::from("bbr")
    }

    fn create(
        control: Datapath<T>,
        log_opt: Option<slog::Logger>,
        sock_id: u32,
        init_cwnd: u32,
    ) -> Self {
        let mut s = Self {
            sock_id: sock_id,
            control_channel: control,
            sc: None,
            logger: log_opt,
            rtt_us: 10000,
            bottle_rate: 0,
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting bbr flow"; "sock_id" => sock_id);
        });

        s.sc = s.install_fold();
        match s.control_channel.send_pattern(
            s.sock_id,
            make_pattern!(
                pattern::Event::SetCwndAbs(init_cwnd) => 
                pattern::Event::WaitRtts(1.0) => 
                pattern::Event::Report
            ),
        ) {
            Ok(_) => (),
            Err(e) => {
                s.logger.as_ref().map(|log| {
                    warn!(log, "send_pattern"; "err" => ?e);
                });
            }
        }

        s
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
        let (loss, rtt, rin, rout) = self.get_fields(m);

        self.rtt_us = rtt;
        if self.bottle_rate < rout {
            self.bottle_rate = rout;
            self.send_pattern();
        }

        self.logger.as_ref().map(|log| {
            debug!(log, "measurement"; 
                "loss" => loss,
                "rtt (us)" => self.rtt_us,
                "sndRate (Mbps)" => rin / 125000,
                "rcvRate (Mbps)" => rout / 125000,
                "setRate (Mbps)" => (self.bottle_rate as f32 * 0.95) as u32 / 125000,
            );
        });
    }
}
