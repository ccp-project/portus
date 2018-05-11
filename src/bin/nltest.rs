#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate portus;

use slog::Drain;

/// If ccp.ko is loaded, return false.
#[cfg(all(target_os = "linux"))] // netlink is linux-only
fn test_ccp_present(log: &slog::Logger) -> bool {
    use std::process::Command;

    let lsmod = Command::new("lsmod")
        .output()
        .expect("lsmod failed");
    debug!(log, "lsmod");
    let loaded_modules = String::from_utf8_lossy(&lsmod.stdout);
    loaded_modules.split('\n').filter(|s| s.contains("ccp")).collect::<Vec<_>>().is_empty()
}

#[cfg(all(target_os = "linux"))] // netlink is linux-only
fn test(log: &slog::Logger) {
    use std::process::Command;
    use portus::ipc::{Backend, Blocking};
    use portus::test_helper::TestMsg;
    use portus::serialize::AsRawMsg;
    use std::sync::{Arc, atomic};

    if !test_ccp_present(log) {
        warn!(log, "ccp.ko loaded, aborting test");
        return;
    }

    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");

    // make clean
    let mkcl = Command::new("make")
        .arg("clean")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");
    debug!(log, "make clean");
    trace!(log, "make clean"; "output" => ?String::from_utf8_lossy(&mkcl.stdout));

    // compile kernel module
    let mk = Command::new("make")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");
    debug!(log, "make");
    trace!(log, "make"; "output" => ?String::from_utf8_lossy(&mk.stdout));

    use std::thread;
    let (tx, rx) = std::sync::mpsc::channel::<bool>();

    // listen
    let listen_log = log.clone();
    let c1 = thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut b = portus::ipc::netlink::Socket::<Blocking>::new()
            .map(|sk| Backend::new(sk, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]))
            .expect("ipc initialization");
        let sender = b.sender();
        debug!(listen_log, "listen");
        tx.send(true).expect("sync");

        if let portus::serialize::Msg::Other(raw) =
            b.next().expect("get message from iterator")
        {
            assert_eq!(TestMsg::from_raw_msg(raw).expect("get TestMsg"), TestMsg(String::from("hello, netlink")));
        }

        debug!(listen_log, "send");
        let msg = TestMsg(String::from("hello, kernel"));
        let test = msg.clone();
        let buf = portus::serialize::serialize(&msg).expect("serialize");
        sender.send_msg(&buf[..]).expect("send response");

        if let portus::serialize::Msg::Other(raw) =
            b.next().expect("get message from iterator")
        {
            let got = TestMsg::from_raw_msg(raw).expect("get from raw");
            assert_eq!(got, test);
        } else {
            panic!("wrong type");
        }
    });

    // load kernel module
    rx.recv().expect("sync");
    debug!(log, "insmod");
    Command::new("sudo")
        .arg("insmod")
        .arg("./src/ipc/test-nl-kernel/nltest.ko")
        .output()
        .expect("insmod failed");

    c1.join().expect("join netlink thread");

    debug!(log, "rmmod");
    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");
    info!(log, "nltest ok");
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
