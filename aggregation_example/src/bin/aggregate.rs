extern crate clap;
use clap::Arg;
extern crate time;
#[macro_use]
extern crate slog;

extern crate ccp_aggregation_example;
extern crate portus;

use ccp_aggregation_example::AggregationExample;
use portus::ipc::{BackendBuilder, Blocking};

fn make_args() -> Result<(ccp_aggregation_example::AggregationExampleConfig, String), String> {
    let matches = clap::App::new("CCP Aggregation Example")
        .version("0.1.0")
        .author("Frank Cangialosi <frankc@csail.mit.edu>")
        .about("Example of host-level aggregation")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix|char)")
             .takes_value(true)
             .required(true)
             .validator(portus::algs::ipc_valid))
        .get_matches();

    Ok((
        ccp_aggregation_example::AggregationExampleConfig::default(),
        String::from(matches.value_of("ipc").unwrap()),
    ))

}

fn main() {
    let log = portus::algs::make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or_default();

    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::<Blocking>::new("in", "out")
                .map(|sk| BackendBuilder{sock: sk})
                .expect("unix ipc initialization");
            portus::run_aggregator::<_, AggregationExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::<Blocking>::new()
                .map(|sk| BackendBuilder{sock: sk})
                .expect("netlink ipc initialization");
            portus::run_aggregator::<_, AggregationExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            ).unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "char" => {
            use portus::ipc::kp::Socket;
            let b = Socket::<Blocking>::new()
                .map(|sk| BackendBuilder {sock: sk})
                .expect("char ipc initialization");
            portus::run_aggregator::<_, AggregationExample<_>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            ).unwrap()
        }
        _ => unreachable!(),
    }
            
}
