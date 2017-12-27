#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
extern crate portus;

use slog::Drain;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
struct TestMsg(String);

use std::io::prelude::*;
impl portus::serialize::AsRawMsg for TestMsg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (0xff, portus::serialize::HDR_LENGTH + self.0.len() as u8, 0)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> portus::Result<()> {
        w.write_all(self.0.as_bytes())?;
        Ok(())
    }

    fn from_raw_msg(msg: portus::serialize::RawMsg) -> portus::Result<Self> {
        let b = msg.get_bytes()?;
        let got = std::str::from_utf8(&b[..]).expect("parse message to str");
        Ok(TestMsg(String::from(got)))
    }
}

#[cfg(all(target_os = "linux"))] // netlink is linux-only
fn test(log: slog::Logger) {
    use std::process::Command;
    use portus::ipc::Backend;
    use portus::serialize::AsRawMsg;

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
        let b = portus::ipc::netlink::Socket::new()
            .and_then(|sk| Backend::new(sk))
            .expect("ipc initialization");
        let rx = b.listen();
        debug!(listen_log, "listen");
        tx.send(true).expect("sync");
        let msg = rx.recv().expect("receive message");
        let got = std::str::from_utf8(&msg[..]).expect("parse message to str");
        assert_eq!(got, "hello, netlink\0\0"); // word aligned

        debug!(listen_log, "send");
        let msg = TestMsg(String::from("hello, kernel\0\0\0\0\0")); // word aligned
        let test = msg.clone();
        let buf = portus::serialize::serialize(msg).expect("serialize");
        b.send_msg(None, &buf[..]).expect("send response");

        let echo = rx.recv().expect("receive echo");
        if let portus::serialize::Msg::Other(raw) =
            portus::serialize::Msg::from_buf(&echo[..]).expect("parse error")
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
fn test(log: slog::Logger) {
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
    test(log);
}
