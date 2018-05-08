// Helper methods for making algorithm binaries.
extern crate slog;
extern crate slog_async;
extern crate slog_term;
use slog::Drain;

use std::result::Result;

pub fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

// Must take a String so that clap::Args::validator will be happy
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
#[cfg(all(target_os = "linux"))]
pub fn ipc_valid(v: String) -> Result<(), String> {
    match v.as_str() {
        "netlink" | "unix" | "char" => Ok(()),
        _ => Err(format!("ipc must be one of (netlink|unix|char): {:?}", v)),
    }
}

// Must take a String so that clap::Args::validator will be happy
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
#[cfg(not(target_os = "linux"))]
pub fn ipc_valid(v: String) -> Result<(), String> {
    match v.as_str() {
        "unix" => Ok(()),
        _ => Err(format!("ipc must be one of (unix): {:?}", v)),
    }
}
