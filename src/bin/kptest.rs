#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate portus;

use slog::Drain;

#[cfg(all(target_os = "linux"))] // netlink is linux-only
fn test(log: &slog::Logger) {
    use std::process::Command;

    debug!(log, "unload module");
    Command::new("sudo")
        .arg("./ccpkp_unload")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("unload failed");

    // make clean
    debug!(log, "make clean");
    let mkcl = Command::new("make")
        .arg("clean")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("make failed to start");
    trace!(log, "make clean"; "output" => ?String::from_utf8_lossy(&mkcl.stdout));

    // compile kernel module
    debug!(log, "make");
    let mk = Command::new("make")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("make failed to start");
    trace!(log, "make"; "output" => ?String::from_utf8_lossy(&mk.stdout));

    debug!(log, "load module");
    Command::new("sudo")
        .arg("./ccpkp_load")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("load failed");

    let output = Command::new("sudo")
        .arg("python")
        .arg("test.py")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("test failed");
    if output.status.success() {
        info!(log, "kptest ok");
    } else {
        println!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        warn!(log, "kptest FAILED");
    }

    debug!(log, "unload module");
    Command::new("sudo")
        .arg("./ccpkp_unload")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("unload failed");
}

#[cfg(not(target_os = "linux"))] // netlink is linux-only
fn test(log: &slog::Logger) {
    warn!(log, "netlink only works on linux.");
    return;
}

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

fn main() {
    let log = make_logger();
    test(&log);
}
