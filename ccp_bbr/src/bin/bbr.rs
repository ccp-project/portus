extern crate clap;
use clap::Arg;
extern crate time;
#[macro_use]
extern crate slog;

extern crate ccp_bbr;
#[macro_use]
extern crate portus;

use ccp_bbr::Bbr;
use portus::ipc::Backend;

fn make_args() -> Result<(ccp_bbr::BbrConfig, String), String> {
    let probe_rtt_interval_default = format!("{}", ccp_bbr::PROBE_RTT_INTERVAL_SECONDS);
    let matches = clap::App::new("CCP BBR")
        .version("0.1.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Implementation of BBR Congestion Control")
        .arg(Arg::with_name("ipc")
             .long("ipc")
             .help("Sets the type of ipc to use: (netlink|unix)")
             .default_value("unix")
             .validator(portus::algs::ipc_valid))
        .arg(Arg::with_name("probe_rtt_interval")
             .long("probe_rtt_interval")
             .help("Sets the BBR probe RTT interval in seconds, after which BBR drops its congestion window to potentially observe a new minimum RTT.")
             .default_value(&probe_rtt_interval_default))
        .get_matches();


    let probe_rtt_interval_arg = time::Duration::seconds(i64::from_str_radix(
        matches.value_of("probe_rtt_interval").unwrap(),
        10,
    ).map_err(|e| format!("{:?}", e))
        .and_then(|probe_rtt_interval_arg| if probe_rtt_interval_arg <= 0 {
            Err(format!(
                "probe_rtt_interval must be positive: {}",
                probe_rtt_interval_arg
            ))
        } else {
            Ok(probe_rtt_interval_arg)
        })?);

    Ok((
        ccp_bbr::BbrConfig {
            probe_rtt_interval: probe_rtt_interval_arg,
        },
        String::from(matches.value_of("ipc").unwrap()),
    ))
}

make_alg_main!(make_args, "BBR", Bbr);
