extern crate clap;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

use std::vec::Vec;
use portus::{Aggregator, CongAlg, Config, Datapath, DatapathInfo, Measurement};
use portus::pattern;
use portus::ipc::Ipc;
use portus::lang::Scope;

pub struct AggregationExample<T: Ipc> {
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    cwnd: u32,
    curr_cwnd_reduction: u32,
    init_cwnd: u32,
    ss_thresh: u32,
    sub_flows: Vec<(u32, Datapath<T>)>,
}

pub const DEFAULT_SS_THRESH: u32 = 0x7fffffff;

#[derive(Clone)]
pub struct AggregationExampleConfig {}

impl Default for AggregationExampleConfig {
    fn default() -> Self {
        AggregationExampleConfig{}
    }
}

#[derive(PartialEq, Eq, Hash)]
#[derive(Clone, Copy)]
pub struct AggregationExampleKey(u32);

impl From<DatapathInfo> for AggregationExampleKey {
    /// Aggregate all flows from this IP.
    /// A more complete implementation might also consider the destination IP in its heuristic
    /// for whether the flows share a bottleneck.
    fn from(d: DatapathInfo) -> Self {
        AggregationExampleKey(d.src_ip)
    }
}

impl<T: Ipc> Aggregator<T> for AggregationExample<T> {
    type Key = AggregationExampleKey;
    
    fn new_flow(&mut self, info: DatapathInfo, control: Datapath<T>) {
        self.install_fold(info.sock_id, &control);
        self.sub_flows.push((info.sock_id, control));
        self.send_pattern();
    }
}

impl<T: Ipc> CongAlg<T> for AggregationExample<T> {
    type Config = AggregationExampleConfig;

    fn name() -> String {
        String::from("aggregation")
    }

    fn create(control: Datapath<T>, cfg: Config<T, AggregationExample<T>>, info: DatapathInfo) -> Self {
        let mut s = Self {
            logger: cfg.logger,
            cwnd: info.init_cwnd,
            init_cwnd: info.init_cwnd,
            curr_cwnd_reduction: 0,
            ss_thresh: DEFAULT_SS_THRESH,
            sc: None,
            sub_flows: vec![],
        };

        s.sc = s.install_fold(info.sock_id, &control);
        s.sub_flows.push((info.sock_id, control));

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"; "flow_sock_id" => info.sock_id);
        });

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
                "curr_cwnd (pkts)" => self.cwnd / 1460, 
                "inflight (pkts)" => inflight, 
                "loss" => loss, 
                "rtt" => rtt,
            );
        });
    }
}

impl<T: Ipc> AggregationExample<T> {
    /* Function that determines congestion window per flow */
    /* For now, simplistically just divide the overall cwnd by the number of */
    /* flows. Can only be called when the connection vector is non-empty */
    fn get_window(&self) -> u32 {
        // Currently, window is independent of the socket (split evenly)
        self.cwnd / (self.sub_flows.len() as u32)
    }

    /* Patterns are sent repeatedly to all connections that are part of an */
    /* aggregate. Loop over connections */
    fn send_pattern(&self) {
        self.sub_flows.iter().for_each(|flow| {
            let &(sock_id, ref control_channel) = flow;
            let flow_cwnd = self.get_window();
            match control_channel.send_pattern(
                sock_id,
                make_pattern!(
                    pattern::Event::SetCwndAbs(flow_cwnd) => 
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
        });
    }

    /* Install fold once for each connection */
    fn install_fold(&self, sock_id: u32, control_channel: &Datapath<T>) -> Option<Scope> {
        match control_channel.install_measurement(
            sock_id,
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
        self.cwnd += 1448u32 * (new_bytes_acked / self.cwnd);
    }

    fn handle_timeout(&mut self) {
        self.ss_thresh /= 2;
        if self.ss_thresh < self.init_cwnd {
            self.ss_thresh = self.init_cwnd;
        }

        let num_flows = self.sub_flows.len() as u32;
        // Congestion window update to reflect timeout for one flow
        self.cwnd = ((self.cwnd * (num_flows-1)) / num_flows) + self.init_cwnd;
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
        // if loss indicator is nonzero
        // AND the losses in the lossy cwnd have not yet been accounted for
        // OR there is a partial ACK AND cwnd was probing ss_thresh
        if loss > 0 && self.curr_cwnd_reduction == 0 || (acked > 0 && self.cwnd == self.ss_thresh) {
            let num_flows = self.sub_flows.len() as u32;
            self.cwnd -= self.cwnd / (2 * num_flows);
            // self.cwnd /= 2;
            if self.cwnd <= self.init_cwnd {
                self.cwnd = self.init_cwnd;
            }

            self.ss_thresh = self.cwnd;
            self.send_pattern();
        }

        self.curr_cwnd_reduction += sacked + loss;
        self.logger.as_ref().map(|log| {
            info!(log, "loss"; "curr_cwnd (pkts)" => self.cwnd / 1448, "loss" => loss, "sacked" => sacked, "curr_cwnd_deficit" => self.curr_cwnd_reduction);
        });
    }
}
