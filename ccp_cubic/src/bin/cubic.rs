extern crate clap;

#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
use slog::Drain;

extern crate ccp_cubic;
extern crate portus;

use clap::Arg;
use ccp_cubic::Cubic;
use portus::ipc::Backend;

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

#[cfg(all(target_os = "linux"))]
fn ipc_valid(v: String) -> std::result::Result<(), String> {
    match v.as_str() {
        "netlink" | "unix" => Ok(()),
        _ => Err(format!("ipc must be one of (netlink|unix): {:?}", v)),
    }
}

#[cfg(not(target_os = "linux"))]
fn ipc_valid(v: String) -> std::result::Result<(), String> {
    match v.as_str() {
        "unix" => Ok(()),
        _ => Err(format!("ipc must be one of (unix): {:?}", v)),
    }
}

fn make_args() -> Result<(ccp_cubic::CubicConfig, String), std::num::ParseFloatError> {
    let ss_thresh_default = format!("{}", ccp_cubic::DEFAULT_SS_THRESH);
    let matches = clap::App::new("CCP Cubic")
        .version("0.1.0")
        .author("Prateesh Goyal <prateesh@mit.edu>")
        .about("Implementation of Cubic Congestion Control")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix)")
             .default_value("unix")
             .validator(ipc_valid))
        .arg(Arg::with_name("ss_thresh")
             .long("ss_thresh")
             .help("Sets the slow start threshold, in bytes")
             .default_value(&ss_thresh_default))
        .arg(Arg::with_name("init_cwnd")
             .long("init_cwnd")
             .help("Sets the initial congestion window, in bytes. Setting 0 will use datapath default.")
             .default_value("10"))
        .get_matches();

    Ok((
        ccp_cubic::CubicConfig {
            ss_thresh: matches.value_of("ss_thresh").unwrap().parse()?,
            init_cwnd: matches.value_of("init_cwnd").unwrap().parse()?,
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

    info!(log, "starting CCP Cubic");
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Cubic<Socket>>(
                b,
                &portus::Config {
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

    info!(log, "starting CCP Cubic");
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Cubic<Socket>>(
                b,
                &portus::Config {
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

            portus::start::<_, Cubic<Socket>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                },
            );
        }
        _ => unreachable!(),
    }
}
