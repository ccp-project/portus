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

// Function to check if the allocator in the cmdline args is sane
fn allocator_valid(v: String) -> std::result::Result<(), String> {
        match v.as_str() {
        "rr" | "maxmin" | "srpt | prop" => Ok(()),
        _ => Err(String::from(
            format!("allocator must be one of (rr|maxmin|srpt|prop): {:?}", v),
        )),
    }
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
        .arg(Arg::with_name("alloc")
             .long("allocator")
             .short("a")
             .help("Set the window allocator for the aggregate: (rr|maxmin|srpt|prop)")
             .default_value("rr")
             .validator(allocator_valid))
        .arg(Arg::with_name("forecast")
             .long("forecast")
             .short("f")
             .help("Enable demand forecast (forecast is disabled by default)"))
        .get_matches();

    Ok((
        ccp_aggregation_example::AggregationExampleConfig {
            allocator: String::from(matches.value_of("alloc").unwrap()),
            forecast: matches.is_present("forecast"),
        },
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
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
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
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
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
