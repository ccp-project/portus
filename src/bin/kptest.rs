#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate portus;

use portus::test_helper::TestMsg;
use portus::serialize::AsRawMsg;
use slog::Drain;
use std::sync::{Arc, atomic};

#[cfg(all(target_os = "linux"))] // kp is linux-only
fn test(log: &slog::Logger) {
    use std::process::Command;
    use portus::ipc::{Backend, Blocking};

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
    if !output.status.success() {
        panic!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }

    // listen
    let listen_log = log.clone();

    { // make this scope so that b is dropped (and the socket closed), so the unload works
        let mut buf = [0u8; 1024];
        let mut b = portus::ipc::kp::Socket::<Blocking>::new()
            .map(|sk| Backend::new(sk, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]))
            .expect("ipc initialization");
        let sender = b.sender();

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
    }

    debug!(log, "unload module");
    Command::new("sudo")
        .arg("./ccpkp_unload")
        .current_dir("./src/ipc/test-char-dev")
        .output()
        .expect("unload failed");

    info!(log, "kptest ok");
}

#[cfg(not(target_os = "linux"))] // kp is linux-only
fn test(log: &slog::Logger) {
    warn!(log, "The character device only works on linux.");
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
