use std::thread;
use std::vec::Vec;

extern crate bytes;
extern crate portus;
extern crate time;

use bytes::{ByteOrder, LittleEndian};
use portus::ipc::{Backend, Ipc};
use time::Duration;

struct TimeMsg(time::Timespec);

use std::io::prelude::*;
impl portus::serialize::AsRawMsg for TimeMsg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (0xff, portus::serialize::HDR_LENGTH + 8 + 4, 0)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> portus::Result<()> {
        let mut msg = [0u8; 12]; // one i64 and one i32
        LittleEndian::write_i64(&mut msg[0..8], self.0.sec);
        LittleEndian::write_i32(&mut msg[8..12], self.0.nsec);
        w.write_all(&msg[..])?;
        Ok(())
    }

    fn from_raw_msg(msg: portus::serialize::RawMsg) -> portus::Result<Self> {
        let b = msg.get_bytes()?;
        let sec = LittleEndian::read_i64(&b[0..8]);
        let nsec = LittleEndian::read_i32(&b[8..12]);
        Ok(TimeMsg(time::Timespec::new(sec, nsec)))
    }
}

use std::sync::mpsc;
use portus::serialize::AsRawMsg;
fn bench<T: Ipc>(
    b: Backend<T>,
    addr: Option<u16>,
    rx: mpsc::Receiver<Vec<u8>>,
    iter: u32,
) -> Vec<Duration> {
    (0..iter)
        .map(|_| {
            let then = time::get_time();
            let msg = portus::serialize::serialize(TimeMsg(then)).expect("serialize");
            b.send_msg(addr, &msg[..]).expect("send ts");

            let echo = rx.recv().expect("receive echo");
            if let portus::serialize::Msg::Other(raw) =
                portus::serialize::Msg::from_buf(&echo[..]).expect("parse error")
            {
                let then = TimeMsg::from_raw_msg(raw).expect("get time from raw");
                time::get_time() - then.0
            } else {
                panic!("wrong type");
            }
        })
        .collect()
}

#[cfg(all(target_os = "linux"))] // netlink is linux-only
fn netlink(iter: u32) -> Vec<Duration> {
    use std::process::Command;

    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");

    // make clean
    Command::new("make")
        .arg("clean")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");

    // compile kernel module
    Command::new("make")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");

    let (tx, rx) = mpsc::channel::<Vec<Duration>>();

    // listen
    let c1 = thread::spawn(move || {
        let b = portus::ipc::netlink::Socket::new()
            .and_then(|sk| Backend::new(sk))
            .expect("ipc initialization");
        tx.send(vec![]).expect("ok to insmod");
        let rx = b.listen();
        let msg = rx.recv().expect("receive message");
        let got = std::str::from_utf8(&msg[..]).expect("parse message to str");
        assert_eq!(got, "hello, netlink\0\0"); // word aligned

        tx.send(bench(b, None, rx, iter)).expect("report rtts");
    });

    rx.recv().expect("wait to insmod");
    // load kernel module
    Command::new("sudo")
        .arg("insmod")
        .arg("./src/ipc/test-nl-kernel/nltest.ko")
        .output()
        .expect("insmod failed");

    c1.join().expect("join netlink thread");

    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");

    rx.recv().expect("get rtts")
}

#[cfg(not(target_os = "linux"))] // netlink is linux-only
fn netlink(_: u32) -> Vec<Duration> {
    vec![]
}

fn unix(iter: u32) -> Vec<Duration> {
    let (tx, rx) = mpsc::channel::<Vec<Duration>>();
    let (ready_tx, ready_rx) = mpsc::channel::<bool>();

    // listen
    let c1 = thread::spawn(move || {
        let b = portus::ipc::unix::Socket::new(0)
            .and_then(|sk| Backend::new(sk))
            .expect("ipc initialization");
        let rx = b.listen();
        ready_rx.recv().expect("sync");
        tx.send(bench(b, Some(42424), rx, iter)).expect(
            "report rtts",
        );
    });

    // echo-er
    let c2 = thread::spawn(move || {
        let sk = portus::ipc::unix::Socket::new(42424).expect("sk init");
        let mut buf = [0u8; 1024];
        ready_tx.send(true).expect("sync");
        for _ in 0..iter {
            let rcv = sk.recv(&mut buf[..]).expect("recv");
            sk.send(None, rcv).expect("echo");
        }
    });

    c1.join().expect("join thread");
    c2.join().expect("join echo thread");
    rx.recv().expect("get rtts")
}

fn main() {
    if cfg!(target_os = "linux") {
        let nl_rtts: Vec<i64> = netlink(10)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap())
            .collect();
        println!("{:?}", nl_rtts);
    }

    let unix_rtts: Vec<i64> = unix(10)
        .iter()
        .map(|d| d.num_nanoseconds().unwrap())
        .collect();
    println!("{:?}", unix_rtts);
}
