extern crate clap;
use clap::Arg;
extern crate time;
#[macro_use]
extern crate slog;

extern crate ccp_cluster_example;
extern crate portus;

use ccp_cluster_example::ClusterExample;
use portus::ipc::{BackendBuilder, Nonblocking};

fn make_args() -> Result<(ccp_cluster_example::ClusterExampleConfig, String, String), String> {
    let matches = clap::App::new("Cluster CCP Slave")
        .version("0.1.0")
        .author("Frank Cangialosi <frankc@csail.mit.edu>")
        .about("Example of host that participates in cluster-level congestion control")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix|char)")
             .takes_value(true)
             .required(true)
             .validator(portus::algs::ipc_valid))
        .arg(Arg::with_name("controller")
             .long("controller")
             .help("ip:port of controller")
             .takes_value(true)
             .required(true))
        .get_matches();

    Ok((
        ccp_cluster_example::ClusterExampleConfig::default(),
        String::from(matches.value_of("ipc").unwrap()),
        String::from(matches.value_of("controller").unwrap()),
    ))

}

fn main() {
    let log = portus::algs::make_logger();
    let (cfg, ipc, controller_addr) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or_default();
    let local_addr = String::from("0.0.0.0:4052");

    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::<Nonblocking>::new("in", "out")
                .map(|sk| BackendBuilder{sock: sk})
                .expect("unix ipc initialization");
            portus::run_aggregator_with_remote::<_, ClusterExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
                local_addr,
                controller_addr,

            ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::<Nonblocking>::new()
                .map(|sk| BackendBuilder{sock: sk})
                .expect("netlink ipc initialization");
            portus::run_aggregator_with_remote::<_, ClusterExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
                local_addr,
                controller_addr,
            ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "char" => {
            use portus::ipc::kp::Socket;
            let b = Socket::<Nonblocking>::new()
                .map(|sk| BackendBuilder {sock: sk})
                .expect("char ipc initialization");
            portus::run_aggregator_with_remote::<_, ClusterExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
                local_addr,
                controller_addr,
            ).unwrap()
        }
        _ => unreachable!(),
    }
            
}
