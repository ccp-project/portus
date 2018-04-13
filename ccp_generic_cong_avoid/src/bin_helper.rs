use clap;
use clap::Arg;
use {DEFAULT_SS_THRESH, GenericCongAvoid, GenericCongAvoidAlg, GenericCongAvoidConfig, GenericCongAvoidConfigSS, GenericCongAvoidConfigReport};
use portus;
use portus::ipc::{Backend, ListenMode};
use slog;
use std;
use time;

pub fn make_args(name: &str) -> Result<(GenericCongAvoidConfig, String), std::num::ParseIntError> {
    let ss_thresh_default = format!("{}", DEFAULT_SS_THRESH);
    let matches = clap::App::new(name)
        .version("0.2.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("CCP implementation of a congestion avoidance algorithm")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix)")
             .default_value("unix")
             .validator(portus::algs::ipc_valid))
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
             .help("Implement slow start in the datapath"))
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
        .arg(Arg::with_name("compensate_update")
             .long("compensate_update")
             .help("Scale the congestion window update to compensate for reporting delay"))
        .get_matches();

    Ok((
        GenericCongAvoidConfig {
            ss_thresh: u32::from_str_radix(matches.value_of("ss_thresh").unwrap(), 10)?,
            init_cwnd: u32::from_str_radix(matches.value_of("init_cwnd").unwrap(), 10)?,
            report: if matches.is_present("report_per_ack") {
                GenericCongAvoidConfigReport::Ack
            } else if matches.is_present("report_per_interval") {
                GenericCongAvoidConfigReport::Interval(
                    time::Duration::milliseconds(matches
                        .value_of("report_per_interval")
                        .unwrap()
                        .parse()
                        .unwrap()
                    )
                )
            } else {
                GenericCongAvoidConfigReport::Rtt
            },
            ss: if matches.is_present("ss_in_fold") {GenericCongAvoidConfigSS::Datapath} else {GenericCongAvoidConfigSS::Ccp},
            use_compensation: matches.is_present("compensate_update"),
        },
        String::from(matches.value_of("ipc").unwrap()),
    ))
}

pub fn start<T: GenericCongAvoidAlg>(ipc: &str, log: slog::Logger, cfg: GenericCongAvoidConfig) {
    match ipc {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out")
                .map(|sk| Backend::new(sk, ListenMode::Blocking))
                .expect("ipc initialization");
            portus::start::<_, GenericCongAvoid<_, T>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            );
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::new()
                .map(|sk| Backend::new(sk, ListenMode::Blocking))
                .expect("ipc initialization");
            portus::start::<_, GenericCongAvoid<_, T>>(
                b,
                &portus::Config {
                    logger: Some(log),
                    config: cfg,
                }
            );
        }
        _ => unreachable!(),
    }
}
