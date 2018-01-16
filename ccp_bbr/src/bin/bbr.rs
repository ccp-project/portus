extern crate clap;
use clap::Arg;
extern crate time;

#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
use slog::Drain;

extern crate ccp_bbr;
extern crate portus;
use ccp_bbr::Bbr;
use portus::ipc::Backend;

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

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
             .validator(portus::ipc_valid))
        .arg(Arg::with_name("probe_rtt_interval")
             .long("probe_rtt_interval")
             .help("Sets the BBR probe RTT interval in seconds, after which BBR drops its congestion window to potentially observe a new minimum RTT.")
             .default_value(&probe_rtt_interval_default))
        .get_matches();


    let probe_rtt_interval_arg = time::Duration::seconds(i64::from_str_radix(
        matches.value_of("probe_rtt_interval").unwrap(),
        10,
    ).map_err(|e| String::from(format!("{:?}", e)))
        .and_then(|probe_rtt_interval_arg| if probe_rtt_interval_arg <= 0 {
            Err(String::from(format!(
                "probe_rtt_interval must be positive: {}",
                probe_rtt_interval_arg
            )))
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

#[cfg(not(target_os = "linux"))]
fn main() {
    let log = make_logger();
    let (cfg, ipc) = make_args()
        .map_err(|e| warn!(log, "bad argument"; "err" => ?e))
        .unwrap_or(Default::default());

    info!(log, "starting CCP BBR"; "ipc" => ipc.clone());
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new(0).and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Bbr<Socket>>(
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

    info!(log, "starting CCP BBR"; "ipc" => ipc.clone());
    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::new(0).and_then(|sk| Backend::new(sk)).expect(
                "ipc initialization",
            );

            portus::start::<_, Bbr<Socket>>(
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

            portus::start::<_, Bbr<Socket>>(
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
