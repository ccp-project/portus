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
use ccp_generic_cong_avoid::{GenericCongAvoid, GenericCongAvoidConfig, reno::Reno, cubic::Cubic};

pub struct HostAggregator<T: Ipc, C: CongAlg<T>> {
    logger: Option<slog::Logger>,

    num_flows: u32,
    total_cwnd: u32,
    subflows: HashMap<u32, Datapath<T> >,
}

#[derive(Clone)]
pub struct HostAggregatorConfig {
    pub algorithm: String,
}

impl Default for HostAggregatorConfig {
    fn default() -> Self {
        HostAggregatorConfig {
            algorithm : String::new("reno"),
        }
    }
}

#[derive(PartialEq, Eq, Hash)]
#[derive(Clone, Copy)]
pub struct BottleneckID(u32);
impl From<DatapathInfo> for BottleneckID {
    // Just uses source IP address for now, but should eventually use a better method of
    // determining which flows share the same bottleneck.
    fn from(d: DatapathInfo) -> Self {
        BottleneckID(d.src_ip)
    }
}

impl<T: Ipc> Aggregator<T> for HostAggregator<T> {
    type Key = BottleneckID;

    fn new_flow(&mut self, control: Datapath<T>, info: DatapathInfo) {
        self.subflows.insert(info.sock_id, control);
        self.num_flows += 1;
        // give fraction of cwnd? 
        // install / set datapath program
    }

    fn close_one(&mut self, key: BottleneckID) {
        // TODO logic
    }
}

impl<T: Ipc> CongAlg<T> for HostAggregator<T> {
    type Config = HostAggregatorConfig;

    fn name() -> String {
        // TODO add name of base algorithm
        String::from("aggregation+")
    }

    fn create(control: Datapath<T>, cfg: Config<T, HostAggregator<T>>, info: DatapathInfo) -> Self {
        let alg = match cfg.algorithm {
            "reno"  => { GenericCongAvoid::<T, Reno>::create(control, cfg, info) },
            "cubic" => { GenericCongAvoid::<T, Cubic>::create(control, cfg, info) },
            _       => panic!("unsupported algorithm"),
        };
        let mut s = Self {
            logger: cfg.logger,
            cc: Box::new(alg),
            cwnd: info.init_cwnd,
            subflow: HashMap::new(),
            num_flows: 1,
        };
        
        s.new_flow(control, info);

        s.logger.as_ref().map(|log| {
            debug!(log, "starting new aggregate"); 
        });
    
        s
    }

    fn on_report(&mut self, sock_id: u32, m: Report) {

    }
}

impl<T: Ipc> HostAggregator<T> {
    // TODO fn get_fields(&mut self, m: &Report) -> ... {
}
