extern crate clap;

#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
use slog::Drain;

extern crate ccp_aggregation_example;
extern crate portus;

use clap::Arg;
use ccp_aggregation_example::AggregationExample;
use portus::ipc::Backend;

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

fn make_args() -> Result<(ccp_aggregation_example::AggregationExampleConfig, String), std::num::ParseIntError> {
    let matches = clap::App::new("CCP Example Aggregator")
        .version("0.0.1")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Implementation of Aggregating Congestion Control")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix)")
             .default_value("unix")
             .validator(portus::ipc_valid))
        .get_matches();

    Ok((
        ccp_aggregation_example::AggregationExampleConfig{},
        String::from(matches.value_of("ipc").unwrap()),
    ))
}

#[cfg(not(target_os = "linux"))]
fn main() {
    let log = make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or(Default::default());

    info!(log, "starting Aggregating CCP");
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new(0).and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start_aggregator::<_, AggregationExample<Socket>>(
                b,
                portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
            );
        }
        _ => unreachable!(),
    }
}

#[cfg(all(target_os = "linux"))]
fn main() {
    let log = make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or(Default::default());

    info!(log, "starting Aggregating CCP");
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new(0).and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start_aggregator::<_, AggregationExample<Socket>>(
                b,
                portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
            );
        }
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::new().and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start_aggregator::<_, AggregationExample<Socket>>(
                b,
                portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
            );
        }
        _ => unreachable!(),
    }
}
