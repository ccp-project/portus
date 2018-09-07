#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
extern crate portus;
extern crate cluster_message_types;

use std::cmp::max;
use std::time::Instant;
use std::collections::HashMap;
use portus::{Slave, Aggregator, CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use cluster_message_types::{summary::Summary, allocation::Allocation};

pub mod qdisc;
use qdisc::*;

pub const DEFAULT_SS_THRESH: u32 = 0x7fffffff;
pub const DEFAULT_PENDING_BYTES: u32 = 2896;
pub const RTT_EWMA_ALPHA: f64 = 0.1;

pub struct ClusterExample<T: Ipc> {
    logger: Option<slog::Logger>,
    sc: Scope,
    cwnd: u32,
    rate: u32,
    burst: u32,
    init_cwnd: u32,
    mss: u32,

    id: u32,
    min_rtt: u32,
    bytes_acked: u32,
    rtt: u32,
    drops: u32,

    num_flows: u32,
    subflows: HashMap<u32, SubFlow<T>>,
    algorithm: String,
    allocator: String,
    forecast: bool,
    qdisc : Option<Qdisc>,

    _summary : Summary,
}

pub struct SubFlow<T: Ipc> {
    control: Datapath<T>,
    cwnd: u32,
    rate: u32,
    rtt: u32,
    inflight: u32,
    pending: u32,
    util: u32,
    last_msg: std::time::Instant,
}

#[derive(Clone)]
pub struct ClusterExampleConfig {
    pub algorithm: String,
    pub allocator: String,
    pub forecast: bool,
}

impl Default for ClusterExampleConfig {
    fn default() -> Self {
        ClusterExampleConfig {
            algorithm : String::from("reno"),
            allocator: String::from("rr"),
            forecast: true,
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
#[derive(Clone, Copy, Debug)]
pub struct BottleneckID(u32);
impl From<DatapathInfo> for BottleneckID {
    // TODO Just uses source IP address for now, but should eventually use a
    // better method of determining which flows share the same bottleneck.
    fn from(d: DatapathInfo) -> Self {
        BottleneckID(d.src_ip)
    }
}

impl Default for BottleneckID {
    fn default() -> BottleneckID { BottleneckID(0) }
}

impl<T: Ipc> Aggregator<T> for ClusterExample<T> {
    type Key = BottleneckID;

    fn new_flow(&mut self, control: Datapath<T>, info: DatapathInfo) {
        let f = SubFlow {
            control: control,
            cwnd: info.init_cwnd,
            rtt: 0,
            rate: 0,
            inflight: 0,
            pending: DEFAULT_PENDING_BYTES,
            util: 0,
            last_msg: Instant::now(),
        };
        /*
j       let sc = f.set_datapath_program();
        if self.num_flows == 0 {
            self.sc = sc;
        }
        self.subflows.insert(info.sock_id, f);
        self.num_flows += 1;
        */
        self.num_flows += 1;
        if self.num_flows == 1 {
            self.sc = f.set_datapath_program();
            self.reallocate();
        } else {
            self.reallocate();
            f.set_datapath_program();
        }
        self.subflows.insert(info.sock_id, f);
    }

    fn close_one(&mut self, sock_id: u32) {
        self.num_flows -= 1;
        self.subflows.remove(&sock_id);
        self.reallocate();
    }
}

impl<T: Ipc> CongAlg<T> for ClusterExample<T> {
    type Config = ClusterExampleConfig;

    fn name() -> String {
        String::from("cluster-aggregation")
    }

    fn init_programs() -> Vec<(String, String)> {
        vec![(
            String::from("default"), String::from("
                (def (Report
                    (volatile acked 0)
                    (volatile sacked 0)
                    (volatile loss 0)
                    (volatile timeout false)
                    (volatile rtt 0)
                    (volatile inflight 0)
                    (volatile pending 0)
                ))
                (when true 
                    (:= Report.inflight Flow.packets_in_flight)
                    (:= Report.rtt Flow.rtt_sample_us)
                    (:= Report.acked (+ Report.acked Ack.bytes_acked))
                    (:= Report.sacked (+ Report.sacked Ack.packets_misordered))
                    (:= Report.loss Ack.lost_pkts_sample)
                    (:= Report.timeout Flow.was_timeout)
                    (:= Report.pending Flow.bytes_pending)
                    (fallthrough)
                )
                (when (|| Report.timeout (> Report.loss 0))
                    (report)
                    (:= Micros 0)
                )
                (when (> Micros Flow.rtt_sample_us)
                    (report)
                    (:= Micros 0)
            ")
        )]
    }
    fn create(control: Datapath<T>, cfg: Config<T, ClusterExample<T>>, info: DatapathInfo) -> Self {

        let mut s = Self {
            id: info.src_ip,
            logger: cfg.logger,
            cwnd: 500, // info.init_cwnd,
            rate: 0,
            burst: 0,
            init_cwnd: info.init_cwnd,
            sc: Default::default(),
            mss: info.mss,

            min_rtt: 0,
            bytes_acked: 0,
            rtt: 0,
            drops: 0,


            subflows: HashMap::new(),
            num_flows: 0,
            algorithm: cfg.config.algorithm,
            allocator: cfg.config.allocator,
            forecast: cfg.config.forecast,
            qdisc: None,
            _summary : Summary { id: info.src_ip, ..Default::default() }, 
        };

				if s.allocator.as_str() == "qdisc" {
					s.qdisc = Some(Qdisc::get(String::from("ifb0"), (1,0)));
				}

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"); 
        });

        s.new_flow(control, info);
    
        s
    }

    fn on_report(&mut self, sock_id: u32, r: Report) {
        let _ = Slave::on_report(self, sock_id, r);
    }

}

impl<T: Ipc> Slave for ClusterExample<T> {
    fn create_summary(&mut self) -> Option<&Summary> {
        // if self.min_rtt == 0 || self.bytes_acked == 0 {
        //     return None;
        // }

				if self.num_flows <= 0 {
					return None;
				}

        self._summary.num_active_flows = self.num_flows;
        self._summary.bytes_acked = self.bytes_acked;
        self._summary.min_rtt = self.min_rtt;
        self._summary.rtt = self.rtt;
        self._summary.num_drop_events = self.drops;

        self.bytes_acked = 0;
        self.rtt = 0;
        self.drops = 0;

        Some(&self._summary)
    }

    fn next_summary_time(&mut self) -> u32 {
        // max(self.min_rtt, 25_000)
				25_000
    }

    fn on_allocation(&mut self, a: &Allocation) {
        self.rate = a.rate;
				println!("rate {}", a.rate);
        // self.cwnd = (f64::from(a.rate) * f64::from(self.min_rtt)/1.0e6) as u32;
				// self.cwnd = 1200000;
				// self.rate = 12750000;
        self.burst = a.burst;
        self.reallocate();
    }

    fn on_report(&mut self, sock_id: u32, r: Report) -> bool {
        let rs = self.get_fields(&r);
        let (acked, sacked, loss, was_timeout, rtt, inflight, pending) = rs;

        if let Some(flow) = self.subflows.get_mut(&sock_id) {
            flow.rtt = rtt;
            flow.inflight = inflight;
            flow.pending = pending;
            flow.util = acked + sacked;
            flow.last_msg = Instant::now();
        }

        if self.min_rtt == 0 || rtt < self.min_rtt {
            self.min_rtt = rtt;
        }
        self.bytes_acked += acked;
        if self.rtt == 0 {
            self.rtt = rtt;
        } else {
            self.rtt = (((1.0-RTT_EWMA_ALPHA) * f64::from(self.rtt)) + (RTT_EWMA_ALPHA * f64::from(rtt))) as u32;
        }
        if loss > 0 {
            self.drops += loss;
        } else if sacked > 0 {
            self.drops += sacked / self.mss;
        }

        // TODO for now ignoring timeouts

/*
        self.logger.as_ref().map(|log| {
            info!(log, "ack";
                "sid" => sock_id,
                "acked" => acked / self.mss,
                "sacked" => sacked,
                "total_cwnd" => self.cwnd / self.mss,
                "total_rate" => f64::from(self.rate) / 1.0e6 * 8.0,
                "inflight" => inflight,
                "loss" => loss,
                "rtt" => rtt,
                "flows" => self.num_flows,
            );
        });
				*/

        (sacked > 0 || loss > 0)
    }

}

impl<T: Ipc> ClusterExample<T> {

    fn reallocate(&mut self) {

        match self.allocator.as_str() {
            "rr"    => self.allocate_rr(),
            "qdisc" => self.allocate_qdisc(),
            _       => unreachable!(),
        }
    }

    fn allocate_qdisc(&mut self) {
        self.logger.as_ref().map(|log| { info!(log, "qdisc.set_rate"; "rate" => self.rate, "bucket" => self.burst) });
        match self.qdisc.as_mut().expect("allocation is qdisc but qdisc is None").set_rate(self.rate, self.burst) {
					Ok(()) => {}
					Err(()) => {eprintln!("ERROR: failed to set rate!!!")}
				}
    }

    fn allocate_rr(&mut self) {
        for (_, f) in &mut self.subflows {
            if self.rate > 0 {
                f.rate = self.rate / self.num_flows;
                f.cwnd = (f64::from(f.rate) * 1.25 * f64::from(self.min_rtt)/1.0e6) as u32;
								self.logger.as_ref().map(|log| {
									info!(log, "setting rate"; "rate"=>f.rate,"cwnd"=>f.cwnd);
								});
                if let Err(e) = f.control.update_field(&self.sc, &[("Cwnd", f.cwnd), ("Rate", f.rate)]) {
                    self.logger.as_ref().map(|log| { warn!(log, "failed to update cwnd and rate"; "err" => ?e); });
                }
            } else {
                f.cwnd = self.cwnd / self.num_flows;
								self.logger.as_ref().map(|log| {
									info!(log, "setting cwnd"; "cwnd"=>f.cwnd);
								});
                if let Err(e) = f.control.update_field(&self.sc, &[("Cwnd", f.cwnd)]) {
                    self.logger.as_ref().map(|log| { warn!(log, "failed to update cwnd and rate"; "err" => ?e); });
                }
            }
        }
    }

    fn get_fields(&mut self, r: &Report) -> (u32, u32, u32, bool, u32, u32, u32) {
        let sc = &self.sc;
        let ack = r.get_field(&String::from("Report.acked"), sc).expect(
            "expected acked field in returned report",
        ) as u32;

        let sack = r.get_field(&String::from("Report.sacked"), sc).expect(
            "expected sacked field in returned report",
        ) as u32;

        let loss = r.get_field(&String::from("Report.loss"), sc).expect(
            "expected loss field in returned report",
        ) as u32;

        let was_timeout = r.get_field(&String::from("Report.timeout"), sc).expect(
            "expected timeout field in returned report",
        ) as u32;

        let rtt = r.get_field(&String::from("Report.rtt"), sc).expect(
            "expected rtt field in returned report",
        ) as u32;

        let inflight = r.get_field(&String::from("Report.inflight"), sc).expect(
            "expected inflight field in returned report",
        ) as u32;

        let pending = r.get_field(&String::from("Report.pending"), sc).expect(
            "expected pending field in returned report",
        ) as u32;

        (ack, sack, loss, was_timeout == 1, rtt, inflight, pending)
    }
}

impl<T: Ipc> SubFlow<T> {
    fn set_datapath_program(&self) -> Scope {
        self.control.set_program(String::from("default"), None).unwrap()
    }
}
