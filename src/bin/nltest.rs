extern crate portus;

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
fn test() {
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
    println!("make clean...");
    println!("{}", String::from_utf8_lossy(&mkcl.stdout));

    // compile kernel module
    let mk = Command::new("make")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");
    println!("make...");
    println!("{}", String::from_utf8_lossy(&mk.stdout));

    use std::thread;

    // listen
    let c1 = thread::spawn(move || {
        let b = portus::ipc::netlink::Socket::new()
            .and_then(|sk| Backend::new(sk))
            .expect("ipc initialization");
        let rx = b.listen();
        println!("listen...");
        let msg = rx.recv().expect("receive message");
        let got = std::str::from_utf8(&msg[..]).expect("parse message to str");
        assert_eq!(got, "hello, netlink\0\0"); // word aligned

        println!("send...");
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
    println!("insmod...");
    Command::new("sudo")
        .arg("insmod")
        .arg("./src/ipc/test-nl-kernel/nltest.ko")
        .output()
        .expect("insmod failed");

    c1.join().expect("join netlink thread");

    println!("rmmod...");
    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");
    println!("\x1B[32m{}\x1B[0m", "nltest ok");
}

#[cfg(not(target_os = "linux"))] // netlink is linux-only
fn test() {
    return;
}

fn main() {
    if !cfg!(target_os = "linux") {
        println!("netlink only works on linux.");
        return;
    }

    test();
}
