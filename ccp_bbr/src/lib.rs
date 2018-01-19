#[macro_use]
extern crate slog;
extern crate time;
#[macro_use]
extern crate portus;

use portus::{CongAlg, Config, Datapath, DatapathInfo, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

/// Linux source: net/ipv4/tcp_bbr.c
///
/// model of the network path:
///    bottleneck_bandwidth = windowed_max(delivered / elapsed, 10 round trips)
///    min_rtt = windowed_min(rtt, 10 seconds)
/// pacing_rate = pacing_gain * bottleneck_bandwidth
/// cwnd = max(cwnd_gain * bottleneck_bandwidth * min_rtt, 4)
///
/// A BBR flow starts in STARTUP, and ramps up its sending rate quickly.
/// When it estimates the pipe is full, it enters DRAIN to drain the queue.
/// In steady state a BBR flow only uses PROBE_BW and PROBE_RTT.
/// A long-lived BBR flow spends the vast majority of its time remaining
/// (repeatedly) in PROBE_BW, fully probing and utilizing the pipe's bandwidth
/// in a fair manner, with a small, bounded queue. *If* a flow has been
/// continuously sending for the entire min_rtt window, and hasn't seen an RTT
/// sample that matches or decreases its min_rtt estimate for 10 seconds, then
/// it briefly enters PROBE_RTT to cut inflight to a minimum value to re-probe
/// the path's two-way propagation delay (min_rtt). When exiting PROBE_RTT, if
/// we estimated that we reached the full bw of the pipe then we enter PROBE_BW;
/// otherwise we enter STARTUP to try to fill the pipe.
///
/// The goal of PROBE_RTT mode is to have BBR flows cooperatively and
/// periodically drain the bottleneck queue, to converge to measure the true
/// min_rtt (unloaded propagation delay). This allows the flows to keep queues
/// small (reducing queuing delay and packet loss) and achieve fairness among
/// BBR flows.
///
/// The min_rtt filter window is 10 seconds. When the min_rtt estimate expires,
/// we enter PROBE_RTT mode and cap the cwnd at bbr_cwnd_min_target=4 packets.
/// After at least bbr_probe_rtt_mode_ms=200ms and at least one packet-timed
/// round trip elapsed with that flight size <= 4, we leave PROBE_RTT mode and
/// re-enter the previous mode. BBR uses 200ms to approximately bound the
/// performance penalty of PROBE_RTT's cwnd capping to roughly 2% (200ms/10s).
///
/// Portus note:
/// This implementation does PROBE_BW and PROBE_RTT, but leaves as future work
/// an implementation of the finer points of other BBR implementations
/// (e.g. policing detection).
pub struct Bbr<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    sock_id: u32,
    probe_rtt_interval: time::Duration,
    bottle_rate: f64,
    min_rtt_us: u32,
    min_rtt_timeout: time::Timespec,
    curr_mode: BbrMode,
}

enum BbrMode {
    ProbeBw,
    ProbeRtt,
}

pub const PROBE_RTT_INTERVAL_SECONDS: i64 = 10;

#[derive(Clone)]
pub struct BbrConfig {
    pub probe_rtt_interval: time::Duration,
    // TODO make more things configurable
}

impl Default for BbrConfig {
    fn default() -> Self {
        BbrConfig { probe_rtt_interval: time::Duration::seconds(PROBE_RTT_INTERVAL_SECONDS) }
    }
}

impl<T: Ipc> Bbr<T> {
    fn send_probe_bw_pattern(&self) {
        self.logger.as_ref().map(|log| {
            debug!(log, "setting pattern"; 
                "cwnd, pkts" => (self.bottle_rate * 2.0 * self.min_rtt_us as f64 / 1e6 / 1460.0) as u32,
                "set rate, Mbps" => self.bottle_rate / 125000.0,
                "up pulse rate, Mbps" => self.bottle_rate * 1.25 / 125000.0,
                "down pulse rate, Mbps" => self.bottle_rate * 0.75 / 125000.0,
            );
        });

        match self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::SetRateAbs((self.bottle_rate * 1.25) as u32) => 
                pattern::Event::SetCwndAbs((self.bottle_rate * 2.0 * self.min_rtt_us as f64 / 1e6) as u32) => 
                pattern::Event::WaitNs(self.min_rtt_us * 1000) => 
                pattern::Event::Report =>
                pattern::Event::SetRateAbs((self.bottle_rate * 0.75) as u32) => 
                pattern::Event::WaitNs(self.min_rtt_us * 1000) => 
                pattern::Event::Report =>
                pattern::Event::SetRateAbs(self.bottle_rate as u32) => 
                pattern::Event::WaitNs(self.min_rtt_us * 6000) => 
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

    fn install_probe_bw_fold(&self) -> Option<Scope> {
        match self.control_channel.install_measurement(
            self.sock_id,
            "
                (def (loss 0) (minrtt +infinity) (rate 0))
                (bind Flow.loss (+ Flow.loss Pkt.lost_pkts_sample))
                (bind Flow.minrtt (min Flow.minrtt Pkt.rtt_sample_us))
                (bind Flow.rate (max Flow.rate (min Pkt.rate_outgoing Pkt.rate_incoming)))
            "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(e) => {
                self.logger.as_ref().map(|log| {
                    warn!(log, "install_fold"; "err" => ?e);
                });
                None
            }
        }
    }

    fn get_probe_bw_fields(&mut self, m: Measurement) -> Option<(u32, u32, f64)> {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let rtt = m.get_field(&String::from("Flow.minrtt"), sc).map(|x| x as u32)?;
        let loss = m.get_field(&String::from("Flow.loss"), sc).map(|x| x as u32)?;
        let rate = m.get_field(&String::from("Flow.rate"), sc).map(|x| x as f64)?;

        Some((loss, rtt, rate))
    }

    fn send_probe_rtt_pattern(&self) {
        match self.control_channel.send_pattern(
            self.sock_id,
            make_pattern!(
                pattern::Event::SetCwndAbs(4 * 1448u32) => 
                pattern::Event::WaitNs(200_000_000) => 
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

    fn install_probe_rtt_fold(&mut self) -> Option<Scope> {
        match self.control_channel.install_measurement(
            self.sock_id,
            "
                (def (minrtt +infinity))
                (bind Flow.minrtt (min Flow.minrtt Pkt.rtt_sample_us))
                (bind isUrgent (< Pkt.packets_in_flight 4))
            "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(e) => {
                self.logger.as_ref().map(|log| {
                    warn!(log, "install_fold"; "err" => ?e);
                });
                None
            }
        }
    }

    fn get_probe_rtt_minrtt(&mut self, m: Measurement) -> u32 {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let rtt = m.get_field(&String::from("Flow.minrtt"), sc).expect(
            "expected minrtt field in returned measurement",
        ) as u32;

        rtt
    }
}

impl<T: Ipc> CongAlg<T> for Bbr<T> {
    type Config = BbrConfig;

    fn name() -> String {
        String::from("bbr")
    }

    fn create(control: Datapath<T>, cfg: Config<T, Bbr<T>>, info: DatapathInfo) -> Self {
        let mut s = Self {
            sock_id: info.sock_id,
            control_channel: control,
            sc: None,
            logger: cfg.logger,
            probe_rtt_interval: cfg.config.probe_rtt_interval,
            bottle_rate: 0.0,
            min_rtt_us: 100000000,
            min_rtt_timeout: time::now().to_timespec() + cfg.config.probe_rtt_interval,
            curr_mode: BbrMode::ProbeBw,
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting bbr flow"; "sock_id" => info.sock_id);
        });

        s.sc = s.install_probe_bw_fold();
        match s.control_channel.send_pattern(
            s.sock_id,
            make_pattern!(
                pattern::Event::SetCwndAbs(info.init_cwnd) => 
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
        match self.curr_mode {
            BbrMode::ProbeRtt => {
                self.min_rtt_us = self.get_probe_rtt_minrtt(m);
                self.sc = self.install_probe_bw_fold();
                self.send_probe_bw_pattern();
                self.curr_mode = BbrMode::ProbeBw;

                self.logger.as_ref().map(|log| {
                    debug!(log, "probe_rtt"; 
                        "min_rtt (us)" => self.min_rtt_us,
                    );
                });
            }
            BbrMode::ProbeBw => {
                let fields = self.get_probe_bw_fields(m);
                if fields.is_none() {
                    return;
                }

                let (loss, minrtt, rate) = fields.unwrap();

                if minrtt < self.min_rtt_us {
                    self.min_rtt_us = minrtt;
                    self.min_rtt_timeout = time::now().to_timespec() + self.probe_rtt_interval;
                }

                if time::now().to_timespec() > self.min_rtt_timeout {
                    self.curr_mode = BbrMode::ProbeRtt;
                    self.sc = self.install_probe_rtt_fold();
                    self.send_probe_rtt_pattern();
                }

                if self.bottle_rate < rate {
                    self.bottle_rate = rate;
                    self.send_probe_bw_pattern();
                }

                self.logger.as_ref().map(|log| {
                    debug!(log, "probe_bw"; 
                        "loss" => loss,
                        "min_rtt (us)" => self.min_rtt_us,
                        "rate (Mbps)" => rate / 125000.0,
                        "setRate (Mbps)" => self.bottle_rate / 125000.0,
                    );
                });
            }
        }
    }
}
