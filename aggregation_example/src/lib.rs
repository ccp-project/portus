extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
extern crate portus;
extern crate ccp_generic_cong_avoid;

use std::collections::HashMap;
use portus::{Aggregator, CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
//use ccp_generic_cong_avoid::{GenericCongAvoid, GenericCongAvoidConfig, reno::Reno, cubic::Cubic};

pub const DEFAULT_SS_THRESH: u32 = 0x7fffffff;
pub const DEFAULT_PENDING_BYTES: u32 = 2896;

pub struct AggregationExample<T: Ipc> {
    logger: Option<slog::Logger>,
    num_flows: u32,
    total_cwnd: u32,
    subflows: HashMap<u32, SubFlow<T> >,
    sc: Scope,
    init_cwnd: u32,
 
    // TODO algorithm, allocator, forecast, maybe set fuction
}

pub struct SubFlow<T: Ipc> {
    control: Datapath<T>,
    cwnd: u32,
    // TODO add other stuff cwnd, rtt etc.
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
            cwnd: info.init_cwnd, // TODO
            // TODO init cc defaults, cwnd, etc.
        };
        let sc = f.install_datapath_program();
        if self.num_flows == 0 {
            self.sc = sc;
        }
        self.subflows.insert(info.sock_id, f);
        self.num_flows += 1;
    }

    fn close_one(&mut self, key: &BottleneckID) {
        // TODO logic
    }
}

impl<T: Ipc> CongAlg<T> for AggregationExample<T> {
    type Config = AggregationExampleConfig;

    fn name() -> String {
        String::from("aggregation")
    }

    fn create(control: Datapath<T>, cfg: Config<T, AggregationExample<T>>, info: DatapathInfo) -> Self {
        /*
        let alg = match cfg.algorithm {
            "reno"  => { GenericCongAvoid::<T, Reno>::create(control, cfg, info) },
            "cubic" => { GenericCongAvoid::<T, Cubic>::create(control, cfg, info) },
            _       => panic!("unsupported algorithm"),
        };
        */

        let mut s = Self {
            logger: cfg.logger,
            total_cwnd: info.init_cwnd,
            init_cwnd: info.init_cwnd,
            sc: Default::default(),
            subflows: HashMap::new(),
            num_flows: 1,
            // TODO allocator, algorithm, forecast
        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"); 
        });

        
        s.new_flow(control, info);
    
        s
    }

    fn on_report(&mut self, sock_id: u32, m: Report) {

    }
}

impl<T: Ipc> AggregationExample<T> {
    fn reallocate(&mut self) {

    }

    fn get_fields(&mut self, r: Report) -> (u32, u32, u32, bool, u32, u32, u32) {
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
            ", Some(&[("Cwnd", self.cwnd)])
        ).unwrap()
    }
}
