extern crate clap;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate portus;

use std::time::Instant;
use std::vec::Vec;
use std::collections::HashMap;
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
    subflow: HashMap<u32, Datapath<T> >,
    subflow_pending: HashMap<u32, u32>,
    subflow_rtt: HashMap<u32, u32>,
    subflow_cwnd: HashMap<u32, u32>,
    subflow_util: HashMap<u32, u32>,
    subflow_inflight: HashMap<u32, u32>,
    subflow_last_msg: HashMap<u32, std::time::Instant>,
    subflow_init_cwnd: HashMap<u32, u32>,
    subflow_curr_cwnd_reduction: HashMap<u32, u32>,
    subflow_ss_thresh: HashMap<u32, u32>,
    num_flows: u32,
    allocator: String,
    forecast: bool,
}

pub const DEFAULT_SS_THRESH: u32 = 0x7fffffff;
pub const DEFAULT_PENDING_BYTES: u32 = 2896;

#[derive(Clone)]
pub struct AggregationExampleConfig {
    pub allocator: String,
    pub forecast: bool,
}

impl Default for AggregationExampleConfig {
    fn default() -> Self {
        AggregationExampleConfig{
            allocator: "rr".to_string(),
            forecast: true,
        }
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
        self.subflow.insert(info.sock_id, control);
        self.subflow_rtt.insert(info.sock_id, 0);
        self.subflow_pending.insert(info.sock_id, DEFAULT_PENDING_BYTES);
        self.subflow_cwnd.insert(info.sock_id, DEFAULT_PENDING_BYTES);
        self.subflow_util.insert(info.sock_id, 0);
        self.subflow_inflight.insert(info.sock_id, 0);
        self.subflow_last_msg.insert(info.sock_id, Instant::now());
        self.subflow_init_cwnd.insert(info.sock_id, info.init_cwnd);
        self.subflow_curr_cwnd_reduction.insert(info.sock_id, 0);
        self.subflow_ss_thresh.insert(info.sock_id, DEFAULT_SS_THRESH);
        self.num_flows += 1;
        self.send_pattern(info.sock_id);
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
            /* Flow-specific congestion control variables for per-flow
             * variant */
            subflow_cwnd: HashMap::new(),
            subflow_init_cwnd: HashMap::new(),
            subflow_curr_cwnd_reduction: HashMap::new(),
            subflow_ss_thresh: HashMap::new(),
            sc: None,
            subflow: HashMap::new(),
            subflow_pending: HashMap::new(),
            subflow_rtt: HashMap::new(),
            subflow_util: HashMap::new(),
            subflow_inflight: HashMap::new(),
            subflow_last_msg: HashMap::new(),
            num_flows: 0,
            allocator: cfg.config.allocator,
            forecast: cfg.config.forecast,
        };

        s.sc = s.install_fold(info.sock_id, &control);
        s.subflow.insert(info.sock_id, control);
        let allocator = s.allocator.clone();
        let forecast  = s.forecast.clone();

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"; "flow_sock_id" => info.sock_id);
            debug!(log, "parameters"; "allocator" => allocator, "forecast" => forecast);
        });

        s.send_pattern(info.sock_id);
        s
    }

    fn measurement(&mut self, sock_id: u32, m: Measurement) {
        let (acked, was_timeout, sacked, loss, rtt, inflight, pending) = self.get_fields(m);

        self.subflow_rtt.insert(sock_id, rtt);
        self.subflow_pending.insert(sock_id, pending);
        self.subflow_util.insert(sock_id, acked+sacked);
        self.subflow_inflight.insert(sock_id, inflight);
        self.subflow_last_msg.insert(sock_id, Instant::now());

        let perflow: bool = self.allocator.as_str() == "perflow";

        if was_timeout {
            self.handle_timeout(sock_id, perflow);
            return;
        }

        // increase the cwnd corresponding to new in-order cumulative ACKs
        self.additive_increase_with_slow_start(acked, sock_id, perflow);

        if loss > 0 || sacked > 0 {
            self.cwnd_reduction(loss, sacked, acked, sock_id, perflow);
        } else if acked < self.curr_cwnd_reduction {
            if perflow {
                let mut curr_cwnd_reduction = *self.subflow_curr_cwnd_reduction.get(&sock_id).unwrap();
                curr_cwnd_reduction -= acked / 1448u32;
                self.subflow_curr_cwnd_reduction.insert(sock_id, curr_cwnd_reduction);
            } else {
                self.curr_cwnd_reduction -= acked / 1448u32;
            }
        } else {
            if perflow {
                self.subflow_curr_cwnd_reduction.insert(sock_id, 0);
            } else {
                self.curr_cwnd_reduction = 0;
            }
        }

        if self.curr_cwnd_reduction > 0 {
            self.logger.as_ref().map(|log| {
                debug!(log, "in cwnd reduction"; "acked" => acked / 1448u32, "deficit" => self.curr_cwnd_reduction);
            });
            return;
        }

        self.send_pattern(sock_id);

        self.logger.as_ref().map(|log| {
            debug!(log, "got ack";
                "flow sock_id" => sock_id,
                "acked(pkts)" => acked / 1448u32, 
                "curr_cwnd (pkts)" => self.cwnd / 1460, 
                "inflight (pkts)" => inflight, 
                "loss" => loss, 
                "rtt" => rtt,
                //"prior_cwnd" => prior_cwnd,
                "acked+sacked" => acked+sacked,
            );
        });
    }
}

impl<T: Ipc> AggregationExample<T> {

    /* Determine average RTT of all the managed connections */
    fn get_average_rtt(&self) -> u32 {
        let mut average = 0;
        if self.num_flows == 0 {
            return 0;
        }
        /* The code below is guaranteed to run when there is at least one flow */
        /* in the aggregate. */
        for (_sock_id, rtt) in &self.subflow_rtt {
            average += rtt;
        }
        return average / self.num_flows;
    }

    fn get_demand_vec(&self) -> Vec<(u32, u32)> {
        let mut demand_vec : Vec<(u32, u32)> = self.
            subflow_pending.clone().into_iter().collect();
        if self.forecast {
            /* Forecast demands based on allocated cwnd and time past since last ack
             * for this flow */
            for i in 0..demand_vec.len() {
                let (sock, demand) = demand_vec[i];
                let cwnd = self.subflow_cwnd.get(&sock).unwrap();
                let rtt_us = self.subflow_rtt.get(&sock).unwrap();
                let dur = self.subflow_last_msg.get(&sock).unwrap().elapsed();
                let elapsed_us =  dur.as_secs() as u32 * 1000000 +
                    (dur.subsec_nanos() / 1000);
                let mut new_demand = demand;
                if *rtt_us > 0 {
                    let mut temp : u64 = *cwnd as u64 * (elapsed_us) as u64;
                    temp /= *rtt_us as u64;
                    if temp as u32 > demand {
                        new_demand = 0;
                    } else {
                        new_demand = demand - (temp as u32);
                    }
                }
                demand_vec[i] = (sock, std::cmp::max(new_demand, DEFAULT_PENDING_BYTES));
            }
        }
        /* Always return a sorted demand vector */
        demand_vec.sort_by(|a, b| { a.1.cmp(&b.1) });
        println!("Adjusted demands {:?} (forecast={:?})", demand_vec, self.forecast);
        demand_vec
    }

    /* Choose the allocator that needs to be used to send control patterns here. */
    fn send_pattern(&mut self, sock: u32) {
        match self.allocator.as_str() {
            "rr" => self.send_pattern_alloc_rr(),
            "maxmin" => self.send_pattern_alloc_maxmin(),
            "srpt" => self.send_pattern_alloc_srpt(),
            "prop" => self.send_pattern_alloc_proportional(),
            "perflow" => self.send_pattern_alloc_perflow(sock),
            _ => unreachable!(),
        };
        self.send_pattern_alloc_messages();
    }

    /* max-min fair allocation based on demands */
    fn send_pattern_alloc_maxmin(&mut self) {
        let demand_vec : Vec<_> = self.get_demand_vec();
        let mut available_cwnd = self.cwnd;
        let mut num_flows_to_allocate = self.num_flows;
        for (sock_id, demand) in demand_vec { // sorted traversal
            if demand < available_cwnd / num_flows_to_allocate {
                self.subflow_cwnd.insert(sock_id, std::cmp::max(demand, DEFAULT_PENDING_BYTES));
                available_cwnd -= demand;
                num_flows_to_allocate -= 1;
            } else {
                self.subflow_cwnd.insert(sock_id, std::cmp::max(
                    available_cwnd / num_flows_to_allocate, DEFAULT_PENDING_BYTES));
            }
        }
        self.send_pattern_alloc_messages();
    }

    /* we allocate the entire cwnd, but proportional to flow demands. */
    fn send_pattern_alloc_proportional(&mut self) {
        let demand_vec : Vec<_> = self.get_demand_vec();
        let total_demand = demand_vec
            .iter().fold(0, |sum, x| { sum + (x.1) });
        if total_demand > 0 {
            for (sock_id, demand) in demand_vec {
                let mut temp: u64 = (self.cwnd as u64) * (demand as u64);
                temp /= total_demand as u64;
                self.subflow_cwnd.insert(sock_id, std::cmp::max(temp as u32, DEFAULT_PENDING_BYTES));
            }
            self.send_pattern_alloc_messages();
        } else { // IF total demand is 0 (almost surely a bug), fall back to RR.
            self.logger.as_ref().map(|log| {
                warn!(log, "alloc_pf found total demand to be zero";);
            });
            self.send_pattern_alloc_rr();
        }
    }

    /* demand-blind round robin allocation */
    fn send_pattern_alloc_rr(&mut self) {
        for (&sock_id, _) in &mut self.subflow_pending {
            self.subflow_cwnd.insert(sock_id, self.cwnd / self.num_flows);
        }
        self.send_pattern_alloc_messages();
    }

    /* Set congestion windows based on remaining demand. Smallest demands get
     * allocated first in full, while larger demands that exceed total
     * available window may not even be allocated. */
    fn send_pattern_alloc_srpt(&mut self) {
        let demand_vec : Vec<_> = self.get_demand_vec();
        /* Must allocate in order of increasing demands, which get_demand_vec()
           return order guarantees */
        let mut allocated_cwnd = 0;
        for (sock_id, flow_demand) in demand_vec {
            let mut flow_cwnd;
            /* Perform allocation in order of demands, keeping larger flows out
             * if necessary */
            if allocated_cwnd < self.cwnd {
                if allocated_cwnd + flow_demand < self.cwnd {
                    flow_cwnd = flow_demand;
                    allocated_cwnd += flow_demand;
                } else {
                    flow_cwnd = self.cwnd - allocated_cwnd;
                    allocated_cwnd = self.cwnd;
                }
            } else {
                flow_cwnd = DEFAULT_PENDING_BYTES; // keep a small number of packets in flight anyway
            }
            self.subflow_cwnd.insert(sock_id, std::cmp::max(flow_cwnd, DEFAULT_PENDING_BYTES));
        }
        self.send_pattern_alloc_messages();
    }

    /* This is a qualitatively different allocation mechanism which tries to
     * schedule in addition to allocating window. Not currently used. */
    fn send_pattern_sched_rr(&mut self) {
        let mut count = 0;
        let num_flows = self.subflow.len() as u32;
        let low_cwnd = 2; // number of packets in "off" phase of RR
        if num_flows == 0 {
            return;
        }
        let rr_interval_ns = self.get_average_rtt() * 1000;
        let flow_cwnd = self.cwnd / self.num_flows;
        for (&sock_id, ref control_channel) in &self.subflow {
            count = count + 1;
            let begin_off_time = rr_interval_ns * (count - 1);
            let end_off_time = rr_interval_ns * (num_flows - count);
            let xmit_time = rr_interval_ns;
            self.logger.as_ref().map(|log| {
                info!(log, "sending"; "begin_off" => begin_off_time, "xmit" => xmit_time, "end_off" => end_off_time);
            });
            match control_channel.send_pattern(
                sock_id,
                make_pattern!(
                    pattern::Event::SetCwndAbs(low_cwnd) =>
                    pattern::Event::WaitNs(begin_off_time) =>
                    pattern::Event::SetCwndAbs(flow_cwnd) =>
                    pattern::Event::WaitNs(xmit_time) =>
                    pattern::Event::SetCwndAbs(low_cwnd) =>
                    pattern::Event::WaitNs(end_off_time) =>
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
        };
    }

    fn send_pattern_alloc_perflow(&mut self, sock: u32) {
        let control_channel = self.subflow.get(&sock).unwrap();
        let flow_cwnd = *self.subflow_cwnd.get(&sock).unwrap();
        match control_channel.send_pattern(
            sock,
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
                ()
            }
        };        
    }    

    fn send_pattern_alloc_messages(&self) {
        println!("Allocated windows {:?}, overall {}", self.subflow_cwnd, self.cwnd);
        for (&sock_id, &flow_cwnd) in &self.subflow_cwnd {
            self.subflow.
                get(&sock_id).
                and_then(|control_channel| {
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
                            ()
                        }
                    };
                    Some(())
                })
                .or_else(|| {
                    Some(())
                });
        }
    }

    /* Install fold once for each connection */
    fn install_fold(&self, sock_id: u32, control_channel: &Datapath<T>) -> Option<Scope> {
        match control_channel.install_measurement(
            sock_id,
            "
                (def (acked 0) (sacked 0) (loss 0) (timeout false) (rtt 0) (inflight 0) (pending 0))
                (bind Flow.inflight Pkt.packets_in_flight)
                (bind Flow.rtt Pkt.rtt_sample_us)
                (bind Flow.acked (+ Flow.acked Pkt.bytes_acked))
                (bind Flow.sacked (+ Flow.sacked Pkt.packets_misordered))
                (bind Flow.loss Pkt.lost_pkts_sample)
                (bind Flow.timeout Pkt.was_timeout)
                (bind Flow.pending Pkt.bytes_pending)
                (bind isUrgent Pkt.was_timeout)
                (bind isUrgent (!if isUrgent (> Flow.loss 0)))
             "
                .as_bytes(),
        ) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }

    fn get_fields(&mut self, m: Measurement) -> (u32, bool, u32, u32, u32, u32, u32) {
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

        let pending = m.get_field(&String::from("Flow.pending"), sc).expect(
            "expected pending field in returned measurement",
        ) as u32;

        (ack, was_timeout == 1, sack, loss, rtt, inflight, pending)
    }

    fn additive_increase_with_slow_start(&mut self, acked: u32,
                                         sock: u32, perflow: bool) {
        let mut new_bytes_acked = acked;
        let mut cwnd = self.cwnd;
        let mut ss_thresh = self.ss_thresh;
        let mut num_flows = self.num_flows;
        if perflow {
            cwnd = *self.subflow_cwnd.get(&sock).unwrap();
            ss_thresh = *self.subflow_ss_thresh.get(&sock).unwrap();
            num_flows = 1;
        }
 
        if cwnd < ss_thresh {
            // increase cwnd by 1 per packet, until ssthresh
            if cwnd + new_bytes_acked > ss_thresh {
                new_bytes_acked -= ss_thresh - cwnd;
                cwnd = ss_thresh;
            } else {
                cwnd += new_bytes_acked;
                new_bytes_acked = 0;
            }
        }

        // increase cwnd by 1 / cwnd per packet
        cwnd += 1448u32 * num_flows * (new_bytes_acked / cwnd);

        if perflow {
            self.subflow_cwnd.insert(sock, cwnd);
        } else {
            self.cwnd = cwnd;
        }
    }

    fn handle_timeout(&mut self, sock: u32, perflow: bool) {
        let mut ss_thresh = self.ss_thresh;
        let mut init_cwnd = self.init_cwnd;
        let mut curr_cwnd_reduction = self.curr_cwnd_reduction;
        let mut cwnd = self.cwnd;
        let mut num_flows = self.subflow.len() as u32;
        if perflow {
            ss_thresh = *self.subflow_ss_thresh.get(&sock).unwrap();
            init_cwnd = *self.subflow_init_cwnd.get(&sock).unwrap();
            curr_cwnd_reduction = *self.subflow_curr_cwnd_reduction.get(&sock).unwrap();
            cwnd = *self.subflow_cwnd.get(&sock).unwrap();
            num_flows = 1;
        }

        ss_thresh /= 2;
        if ss_thresh < init_cwnd {
            ss_thresh = init_cwnd;
        }

        // Congestion window update to reflect timeout for one flow
        cwnd = ((cwnd * (num_flows-1)) / num_flows) + init_cwnd;
        curr_cwnd_reduction = 0;

        if perflow {
            self.subflow_ss_thresh.insert(sock, ss_thresh);
            self.subflow_cwnd.insert(sock, cwnd);
            self.subflow_curr_cwnd_reduction.insert(sock, curr_cwnd_reduction);
        } else {
            self.ss_thresh = ss_thresh;
            self.cwnd = cwnd;
            self.curr_cwnd_reduction = curr_cwnd_reduction;
        }

        self.logger.as_ref().map(|log| {
            warn!(log, "timeout"; 
                "curr_cwnd (pkts)" => cwnd / 1448, 
                "ssthresh" => ss_thresh,
                "flow" => sock,  
            );
        });

        self.send_pattern(sock);
        return;
    }

    /// Handle sacked or lost packets
    /// Only call with loss > 0 || sacked > 0
    fn cwnd_reduction(&mut self, loss: u32, sacked: u32, acked: u32,
                      sock: u32, perflow: bool) {
        // if loss indicator is nonzero
        // AND the losses in the lossy cwnd have not yet been accounted for
        // OR there is a partial ACK AND cwnd was probing ss_thresh
        let mut curr_cwnd_reduction = self.curr_cwnd_reduction;
        let mut num_flows = self.subflow.len() as u32;
        let mut ss_thresh = self.ss_thresh;
        let mut cwnd = self.cwnd;
        let mut init_cwnd = self.init_cwnd;
        if perflow {
            curr_cwnd_reduction = *self.subflow_curr_cwnd_reduction.get(&sock).unwrap();
            num_flows = 1;
            ss_thresh = *self.subflow_ss_thresh.get(&sock).unwrap();
            cwnd = *self.subflow_cwnd.get(&sock).unwrap();
            init_cwnd = *self.subflow_init_cwnd.get(&sock).unwrap();
        }
        
        if loss > 0 && curr_cwnd_reduction == 0 || (acked > 0 && cwnd == ss_thresh) {
            cwnd -= cwnd / (2 * num_flows);
            if cwnd <= init_cwnd {
                cwnd = init_cwnd;
            }

            ss_thresh = cwnd;
            self.send_pattern(sock);
        }

        curr_cwnd_reduction += sacked + loss;

        if perflow {
            self.subflow_cwnd.insert(sock, cwnd);
            self.subflow_ss_thresh.insert(sock, ss_thresh);
            self.subflow_curr_cwnd_reduction.insert(sock, curr_cwnd_reduction);
        } else {
            self.cwnd = cwnd;
            self.ss_thresh = ss_thresh;
            self.curr_cwnd_reduction = curr_cwnd_reduction;
        }
        
        self.logger.as_ref().map(|log| {
            info!(log, "loss";
                  "curr_cwnd (pkts)" => cwnd / 1448,
                  "loss" => loss,
                  "sacked" => sacked,
                  "curr_cwnd_deficit" => curr_cwnd_reduction,
                  "flow" => sock);
        });
    }
}
