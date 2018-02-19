extern crate clap;
#[macro_use]
extern crate slog;

extern crate ccp_cubic;
#[macro_use]
extern crate portus;

use clap::Arg;
use ccp_cubic::Cubic;
use portus::ipc::Backend;

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
             .validator(portus::algs::ipc_valid))
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

make_alg_main!(make_args, "Cubic", Cubic);
