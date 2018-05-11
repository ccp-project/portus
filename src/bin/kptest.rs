#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate portus;

use slog::Drain;

#[cfg(all(target_os = "linux"))] // kp is linux-only
fn test(log: &slog::Logger) {
    use std::process::Command;
    use portus::ipc::{Backend, Blocking};
    use portus::test_helper::TestMsg;
    use portus::serialize::AsRawMsg;
    use std::sync::{Arc, atomic};

    debug!(log, "update ccp-kernel submodule");
    Command::new("git")
        .arg("submodule")
        .arg("update")
        .arg("--init")
        .arg("--recursive")
        .current_dir("./")
        .output()
        .expect("submodule update failed");

    debug!(log, "unload module");
    Command::new("sudo")
        .arg("./ccp_kernel_unload")
        .current_dir("./src/ipc/test-char-dev/ccp-kernel")
        .output()
        .expect("unload failed");

    // make clean
    debug!(log, "make clean");
    let mkcl = Command::new("make")
        .arg("clean")
        .current_dir("./src/ipc/test-char-dev/ccp-kernel")
        .output()
        .expect("make failed to start");
    trace!(log, "make clean ccp-kernel"; "output" => ?String::from_utf8_lossy(&mkcl.stdout));

    // compile kernel module
    debug!(log, "make");
    let mk = Command::new("make")
        .arg("ONE_PIPE=y")
        .current_dir("./src/ipc/test-char-dev/ccp-kernel")
        .output()
        .expect("make failed to start");
    trace!(log, "make ccp-kernel"; "output" => ?String::from_utf8_lossy(&mk.stdout));

    debug!(log, "load module");
    let load = Command::new("sudo")
        .arg("./ccp_kernel_load")
        .arg("ipc=1")
        .current_dir("./src/ipc/test-char-dev/ccp-kernel")
        .output()
        .expect("load failed");
    trace!(log, "./ccp_kernel_load"; "output" => ?String::from_utf8_lossy(&load.stdout));
    let load_stderr = String::from_utf8_lossy(&load.stderr);
    if load_stderr.len() > 0 {
        println!("{}", load_stderr);
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
        .arg("./ccp_kernel_unload")
        .current_dir("./src/ipc/test-char-dev/ccp-kernel")
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
