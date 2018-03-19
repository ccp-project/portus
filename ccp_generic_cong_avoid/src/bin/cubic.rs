extern crate clap;
extern crate time;

#[macro_use]
extern crate slog;

extern crate ccp_generic_cong_avoid;
extern crate portus;

use clap::Arg;
use portus::ipc::{Backend, ListenMode};
use ccp_generic_cong_avoid::cubic::Cubic;
use ccp_generic_cong_avoid::GenericCongAvoid;

fn make_args() -> Result<(ccp_generic_cong_avoid::GenericCongAvoidConfig, String), std::num::ParseIntError> {
    let ss_thresh_default = format!("{}", ccp_generic_cong_avoid::DEFAULT_SS_THRESH);
    let matches = clap::App::new("CCP Cubic")
        .version("0.1.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Implementation of Cubic Congestion Control")
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
        .arg(Arg::with_name("compensate_update")
             .long("compensate_update")
             .help("Scale the congestion window update to compensate for reporting delay"))
        .get_matches();

    Ok((
        ccp_generic_cong_avoid::GenericCongAvoidConfig {
            ss_thresh: u32::from_str_radix(matches.value_of("ss_thresh").unwrap(), 10)?,
            init_cwnd: u32::from_str_radix(matches.value_of("init_cwnd").unwrap(), 10)?,
            report: if matches.is_present("report_per_ack") {
                ccp_generic_cong_avoid::GenericCongAvoidConfigReport::Ack
            } else if matches.is_present("report_per_interval") {
                ccp_generic_cong_avoid::GenericCongAvoidConfigReport::Interval(
                    time::Duration::milliseconds(matches
                        .value_of("report_per_interval")
                        .unwrap()
                        .parse()
                        .unwrap()
                    )
                )
            } else {
                ccp_generic_cong_avoid::GenericCongAvoidConfigReport::Rtt
            },
            ss: if matches.is_present("ss_in_fold") {ccp_generic_cong_avoid::GenericCongAvoidConfigSS::Fold} else if matches.is_present("ss_in_pattern") {ccp_generic_cong_avoid::GenericCongAvoidConfigSS::Pattern} else {ccp_generic_cong_avoid::GenericCongAvoidConfigSS::Ccp},
            use_compensation: matches.is_present("compensate_update"),
        },
        String::from(matches.value_of("ipc").unwrap()),
    ))
}

fn main() {
    let log = portus::algs::make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or_default();

    info!(log, "starting CCP"; 
        "algorithm" => "Cubic",
        "ipc" => ipc.clone(),
        "reports" => ?cfg.report,
    );
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new("in", "out")
                .map(|sk| Backend::new(sk, ListenMode::Blocking))
                .expect("ipc initialization");
            portus::start::<_, GenericCongAvoid<_, Cubic>>(
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
            portus::start::<_, GenericCongAvoid<_, Cubic>>(
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
