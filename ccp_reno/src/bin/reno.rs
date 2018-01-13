extern crate clap;

#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
use slog::Drain;

extern crate ccp_reno;
extern crate portus;

use clap::Arg;
use ccp_reno::Reno;
use portus::ipc::Backend;

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

fn make_args() -> Result<(ccp_reno::RenoConfig, String), std::num::ParseIntError> {
    let ss_thresh_default = format!("{}", ccp_reno::DEFAULT_SS_THRESH);
    let matches = clap::App::new("CCP Reno")
        .version("0.1.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Implementation of Reno Congestion Control")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix)")
             .default_value("unix")
             .validator(portus::ipc_valid))
        .arg(Arg::with_name("ss_thresh")
             .long("ss_thresh")
             .help("Sets the slow start threshold, in bytes")
             .default_value(&ss_thresh_default))
        .arg(Arg::with_name("init_cwnd")
             .long("init_cwnd")
             .help("Sets the initial congestion window, in bytes. Setting 0 will use datapath default.")
             .default_value("0"))
        .get_matches();

    Ok((
        ccp_reno::RenoConfig {
            ss_thresh: u32::from_str_radix(matches.value_of("ss_thresh").unwrap(), 10)?,
            init_cwnd: u32::from_str_radix(matches.value_of("init_cwnd").unwrap(), 10)?,
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

    info!(log, "starting CCP Reno"; "ipc" => ipc.clone());
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Reno<Socket>>(
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

    info!(log, "starting CCP Reno"; "ipc" => ipc.clone());
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Reno<Socket>>(
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

            portus::start::<_, Reno<Socket>>(
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
