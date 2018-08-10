extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
extern crate portus;

use std::time::Instant;
use std::collections::HashMap;
use portus::{Aggregator, CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;

pub const DEFAULT_SS_THRESH: u32 = 0x7fffffff;
pub const DEFAULT_PENDING_BYTES: u32 = 2896;

pub struct AggregationExample<T: Ipc> {
    logger: Option<slog::Logger>,
    sc: Scope,
    cwnd: u32,
    init_cwnd: u32,
    curr_cwnd_reduction: u32,
    mss: u32,
    ss_thresh: u32,

    num_flows: u32,
    subflows: HashMap<u32, SubFlow<T>>,
    algorithm: String,
    allocator: String,
    forecast: bool,
}

pub struct SubFlow<T: Ipc> {
    control: Datapath<T>,
    cwnd: u32,
    rtt: u32,
    inflight: u32,
    pending: u32,
    util: u32,
    last_msg: std::time::Instant,
}

#[derive(Clone)]
pub struct AggregationExampleConfig {
    pub algorithm: String,
    pub allocator: String,
    pub forecast: bool,
}

impl Default for AggregationExampleConfig {
    fn default() -> Self {
        AggregationExampleConfig {
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

impl<T: Ipc> Aggregator<T> for AggregationExample<T> {
    type Key = BottleneckID;

    fn new_flow(&mut self, control: Datapath<T>, info: DatapathInfo) {
        let f = SubFlow {
            control: control,
            cwnd: info.init_cwnd,
            rtt: 0,
            inflight: 0,
            pending: DEFAULT_PENDING_BYTES,
            util: 0,
            last_msg: Instant::now(),
        };
        /*
j       let sc = f.install_datapath_program();
        if self.num_flows == 0 {
            self.sc = sc;
        }
        self.subflows.insert(info.sock_id, f);
        self.num_flows += 1;
        */
        self.num_flows += 1;
        if self.num_flows == 1 {
            self.sc = f.install_datapath_program();
            self.reallocate();
        } else {
            self.reallocate();
            f.install_datapath_program();
        }
        self.subflows.insert(info.sock_id, f);
    }

    fn close_one(&mut self, _key: &BottleneckID) {
        self.num_flows -= 1;
        self.reallocate();
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
            sc: Default::default(),
            mss: info.mss,
            curr_cwnd_reduction: 0,
            ss_thresh: DEFAULT_SS_THRESH,

            subflows: HashMap::new(),
            num_flows: 0,
            algorithm: cfg.config.algorithm,
            allocator: cfg.config.allocator,
            forecast: cfg.config.forecast,
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"); 
        });

        s.new_flow(control, info);
    
        s
    }

    fn on_report(&mut self, sock_id: u32, r: Report) {
        let rs = self.get_fields(&r);
        let (acked, sacked, loss, was_timeout, rtt, inflight, pending) = rs;

        if let Some(flow) = self.subflows.get_mut(&sock_id) {
            flow.rtt = rtt;
            flow.inflight = inflight;
            flow.pending = pending;
            flow.util = acked + sacked;
            flow.last_msg = Instant::now();
        }

		if was_timeout {
            self.logger.as_ref().map(|log| {
                warn!(log, "timeout"; 
                    "sid" => sock_id,
                    "total_cwnd" => self.cwnd / self.mss,
                    "ssthresh" => self.ss_thresh,
                );
            });
            self.handle_timeout();
            return;
        }

        match self.algorithm.as_str() {
            "reno" => self.reno_increase_with_slow_start(acked),
            _ => unreachable!(),
        }
        
        if loss > 0 || sacked > 0 {
            self.cwnd_reduction(acked, sacked, loss);
        } else if acked < self.curr_cwnd_reduction {
            self.curr_cwnd_reduction -= acked / self.mss;
        } else {
            self.curr_cwnd_reduction = 0;
        }

        self.reallocate();

        self.logger.as_ref().map(|log| {
            info!(log, "ack";
                "sid" => sock_id,
                "acked" => acked / self.mss,
                "sacked" => sacked,
                "ssthresh" => self.ss_thresh,
                "total_cwnd" => self.cwnd / self.mss,
                "inflight" => inflight,
                "loss" => loss,
                "rtt" => rtt,
                "flows" => self.num_flows,
                "deficit" => self.curr_cwnd_reduction);
        });
    }
}

impl<T: Ipc> AggregationExample<T> {

    fn handle_timeout(&mut self) {
        self.ss_thresh /= 2;
        if self.ss_thresh < self.init_cwnd {
            self.ss_thresh = self.init_cwnd;
        }

        self.cwnd = ((self.cwnd * (self.num_flows - 1)) / self.num_flows) + self.init_cwnd;
        self.curr_cwnd_reduction = 0;

        self.reallocate();
    }

    fn reno_increase_with_slow_start(&mut self, acked: u32) {
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
        // print!("cwnd {} += {} * {} * ( {} / {} ) = ", self.cwnd, self.mss, self.num_flows, new_bytes_acked, self.cwnd);
        self.cwnd += (self.mss as f64 * (new_bytes_acked as f64 / self.cwnd as f64)) as u32;
        // println!("{}", self.cwnd);
    }

    fn cwnd_reduction(&mut self, acked: u32, sacked: u32, loss: u32) {
        if loss > 0 && self.curr_cwnd_reduction == 0 || (acked > 0 && self.cwnd == self.ss_thresh) {
            // TODO reno only reduction
            self.cwnd -= self.cwnd / 2; // * self.num_flows);
            if self.cwnd <= self.init_cwnd {
                self.cwnd = self.init_cwnd;
            }

            self.ss_thresh = self.cwnd;

            // TODO old code did this BEFORE cwnd update, is that correct? (i think it was a bug)
            self.reallocate(); 
        }

        self.curr_cwnd_reduction += sacked + loss;
    }

    fn reallocate(&mut self) {
        match self.allocator.as_str() {
            "rr" => self.allocate_rr(),
            _ => unreachable!(),
        }
    }

    fn allocate_rr(&mut self) {
        for (_, f) in &mut self.subflows {
            f.cwnd = self.cwnd / self.num_flows;
            if let Err(e) = f.control.update_field(&self.sc, &[("Cwnd", f.cwnd)]) {
                self.logger.as_ref().map(|log| { warn!(log, "failed to update cwnd"; "err" => ?e); });
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
    fn install_datapath_program(&self) -> Scope {
        self.control.install(
            b"
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
                )
            ", None
        ).unwrap()
    }
}
