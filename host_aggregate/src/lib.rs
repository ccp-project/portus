extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
extern crate portus;
extern crate ccp_generic_cong_avoid;

use std::collections::HashMap;
use portus::{Aggregator, CongAlg, Config, Datapath, DatapathInfo, Report};
use portus::ipc::Ipc;
//use portus::lang::Scope;
//use ccp_generic_cong_avoid::{GenericCongAvoid, GenericCongAvoidConfig, reno::Reno, cubic::Cubic};

pub struct HostAggregator<T: Ipc, C: CongAlg<T>> {
    logger: Option<slog::Logger>,
    num_flows: u32,
    total_cwnd: u32,
    subflows: HashMap<u32, SubFlow<T> >,
 
    // TODO algorithm, allocator, forecast, maybe set fuction
}

pub struct SubFlow<T: Ipc> {
    control: Datapath<T>,
}



#[derive(Clone)]
pub struct HostAggregatorConfig {
    pub algorithm: String,
    pub allocator: String,
    pub forecast: bool,
}

impl Default for HostAggregatorConfig {
    fn default() -> Self {
        HostAggregatorConfig {
            algorithm : String::new("reno"),
            allocator: String:new("rr"),
            forecast: true,
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
#[derive(Clone, Copy)]
pub struct BottleneckID(u32);
impl From<DatapathInfo> for BottleneckID {
    // TODO Just uses source IP address for now, but should eventually use a
    // better method of determining which flows share the same bottleneck.
    fn from(d: DatapathInfo) -> Self {
        BottleneckID(d.src_ip)
    }
}

impl<T: Ipc> Aggregator<T> for HostAggregator<T> {
    type Key = BottleneckID;

    fn new_flow(&mut self, control: Datapath<T>, info: DatapathInfo) {
        let f = SubFlow {
            control: control,
        };
        let sc = f.install_datapath_program();
        if (self.num_flows == 0) {
            self.sc = sc;
        }
        self.subflows.insert(info.sock_id, f);
        self.num_flows += 1;
    }

    fn close_one(&mut self, key: BottleneckID) {
        // TODO logic
    }
}

impl<T: Ipc> CongAlg<T> for HostAggregator<T> {
    type Config = HostAggregatorConfig;

    fn name() -> String {
        String::from("aggregation")
    }

    fn create(control: Datapath<T>, cfg: Config<T, HostAggregator<T>>, info: DatapathInfo) -> Self {
        /*
        let alg = match cfg.algorithm {
            "reno"  => { GenericCongAvoid::<T, Reno>::create(control, cfg, info) },
            "cubic" => { GenericCongAvoid::<T, Cubic>::create(control, cfg, info) },
            _       => panic!("unsupported algorithm"),
        };
        */

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"); 
        });

        let mut s = Self {
            logger: cfg.logger,
            cwnd: info.init_cwnd,
            init_cwnd: info.init_cwnd,
            curr_cwnd_reduction: 0,
            ss_thresh: DEFAULT_SS_THRESH,
            sc: None,
            subflow: HashMap::new(),
            num_flows: 1,
            // TODO allocator, algorithm, forecast
        };
        
        s.new_flow(control, info);
    
        s
    }

    fn on_report(&mut self, sock_id: u32, m: Report) {

    }
}

impl<T: Ipc> HostAggregator<T> {
}

impl<T: Ipc> SubFlow<T> {
    fn install_datapath_program() -> Scope {
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
