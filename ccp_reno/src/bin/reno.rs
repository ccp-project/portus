extern crate clap;
extern crate time;

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
             .validator(ipc_valid))
        .arg(Arg::with_name("compensate_update")
             .long("compensate_update")
             .help("Scale the congestion window update to compensate for reporting delay"))
        .arg(Arg::with_name("init_cwnd")
             .long("init_cwnd")
             .help("Sets the initial congestion window, in bytes. Setting 0 will use datapath default.")
             .default_value("0"))
        .arg(Arg::with_name("ss_thresh")
             .long("ss_thresh")
             .help("Sets the slow start threshold, in bytes")
             .default_value(&ss_thresh_default))
        .arg(Arg::with_name("ss_in_fold")
             .long("ss_in_fold")
             .help("Implement slow start in a fold function"))
        .arg(Arg::with_name("ss_in_pattern")
             .long("ss_in_pattern")
             .help("Implement slow start in a send pattern"))
        .group(clap::ArgGroup::with_name("slow_start")
               .args(&["ss_in_fold", "ss_in_pattern"])
               .required(false))
        .arg(Arg::with_name("report_per_ack")
             .long("per_ack")
             .help("Specifies that the datapath should send a measurement upon every ACK"))
        .arg(Arg::with_name("report_per_interval")
             .long("report_interval_ms")
             .short("i")
             .takes_value(true))
        .group(clap::ArgGroup::with_name("interval")
               .args(&["report_per_ack", "report_per_interval"])
               .required(false))
        .get_matches();

    Ok((
        ccp_reno::RenoConfig {
            ss_thresh: u32::from_str_radix(matches.value_of("ss_thresh").unwrap(), 10)?,
            init_cwnd: u32::from_str_radix(matches.value_of("init_cwnd").unwrap(), 10)?,
            report: if matches.is_present("report_per_ack") {
                ccp_reno::RenoConfigReport::Ack
            } else if matches.is_present("report_per_interval") {
                ccp_reno::RenoConfigReport::Interval(
                    time::Duration::milliseconds(matches
                        .value_of("report_per_interval")
                        .unwrap()
                        .parse()
                        .unwrap()
                    )
                )
            } else {
                ccp_reno::RenoConfigReport::Rtt
            },
            ss: if matches.is_present("ss_in_fold") {ccp_reno::RenoConfigSS::Fold} else if matches.is_present("ss_in_pattern") {ccp_reno::RenoConfigSS::Pattern} else {ccp_reno::RenoConfigSS::Ccp},
            use_compensation: matches.is_present("compensate_update"),
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

    info!(log, "starting CCP Reno"; 
          "ipc" => ipc.clone(),
          "reports" => ?cfg.report,
    );
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out").and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Reno<Socket>>(
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

            portus::start::<_, Reno<Socket>>(
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
