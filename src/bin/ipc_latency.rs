use std::thread;
use std::vec::Vec;

extern crate bytes;
extern crate clap;
extern crate portus;
extern crate time;

use bytes::{ByteOrder, LittleEndian};
use clap::Arg;
use portus::ipc::{Backend, BackendSender, Blocking, Ipc, Nonblocking};
use time::Duration;
use std::sync::{Arc, atomic};

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
    b: &BackendSender<T>,
    mut l: Backend<T>,
    iter: u32,
) -> Vec<Duration> {
    (0..iter)
        .map(|_| {
            let then = time::get_time();
            let msg = portus::serialize::serialize(&TimeMsg(then)).expect("serialize");
            b.send_msg(&msg[..]).expect("send ts");

            if let portus::serialize::Msg::Other(raw) =
                l.next().expect("receive echo")
            {
                let then = TimeMsg::from_raw_msg(raw).expect("get time from raw");
                time::get_time() - then.0
            } else {
                panic!("wrong type");
            }
        })
        .collect()
}

macro_rules! netlink_bench {
    ($name: ident, $mode: ident) => (
        #[cfg(target_os = "linux")] // netlink is linux-only
        fn $name(iter: u32) -> Vec<Duration> {
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
                let mut buf = [0u8; 1024];
                let nl = portus::ipc::netlink::Socket::<$mode>::new()
                    .map(|sk| Backend::new(sk, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]))
                    .expect("nl ipc initialization");
                tx.send(vec![]).expect("ok to insmod");
                tx.send(bench(&nl.sender(), nl, iter)).expect("report rtts");
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
        fn $name(_: u32) -> Vec<Duration> {
            vec![]
        }
    )
}

netlink_bench!(netlink_blocking, Blocking);
netlink_bench!(netlink_nonblocking, Nonblocking);


macro_rules! kp_bench {
    ($name: ident, $mode: ident) => (
        #[cfg(target_os = "linux")] // kp is linux-only
        fn $name(iter: u32) -> Vec<Duration> {
            use std::process::Command;
            let (tx, rx) = mpsc::channel::<Vec<Duration>>();

            Command::new("sudo")
                .arg("./ccpkp_unload")
                .current_dir("./src/ipc/test-char-dev")
                .output()
                .expect("unload failed");

            // make clean
            Command::new("make")
                .arg("clean")
                .current_dir("./src/ipc/test-char-dev")
                .output()
                .expect("make failed to start");

            // compile kernel module
            Command::new("make")
                .current_dir("./src/ipc/test-char-dev")
                .output()
                .expect("make failed to start");

            Command::new("sudo")
                .arg("./ccpkp_load")
                .current_dir("./src/ipc/test-char-dev")
                .output()
                .expect("load failed");

            let c1 = thread::spawn(move || {
                let mut receive_buf = [0u8; 1024];
                let kp = portus::ipc::kp::Socket::<$mode>::new()
                    .map(|sk| Backend::new(sk, Arc::new(atomic::AtomicBool::new(true)), &mut receive_buf[..]))
                    .expect("kp ipc initialization");
                tx.send(bench(&kp.sender(), kp, iter)).expect("report rtts");
            });

            c1.join().expect("join kp thread");
            Command::new("sudo")
                .arg("./ccpkp_unload")
                .current_dir("./src/ipc/test-char-dev")
                .output()
                .expect("unload failed");
            rx.recv().expect("get rtts")
        }

        #[cfg(not(target_os = "linux"))] // kp is linux-only
        fn $name(_: u32) -> Vec<Duration> {
            vec![]
        }
    )
}

kp_bench!(kp_blocking, Blocking);
kp_bench!(kp_nonblocking, Nonblocking);


macro_rules! unix_bench {
    ($name: ident, $mode: ident) => (
        fn $name(iter: u32) -> Vec<Duration> {
            let (tx, rx) = mpsc::channel::<Vec<Duration>>();
            let (ready_tx, ready_rx) = mpsc::channel::<bool>();

            // listen
            let c1 = thread::spawn(move || {
                let mut receive_buf = [0u8; 1024];
                let unix = portus::ipc::unix::Socket::<$mode>::new("in", "out")
                    .map(|sk| Backend::new(sk, Arc::new(atomic::AtomicBool::new(true)), &mut receive_buf[..]))
                    .expect("unix ipc initialization");
                ready_rx.recv().expect("sync");
                tx.send(bench(&unix.sender(), unix, iter)).expect(
                    "report rtts",
                );
            });

            // echo-er
            let c2 = thread::spawn(move || {
                let sk = portus::ipc::unix::Socket::<Blocking>::new("out","in")
                    .expect("sk init");
                let mut buf = [0u8; 1024];
                ready_tx.send(true).expect("sync");
                for _ in 0..iter {
                    let rcv = sk.recv(&mut buf[..]).expect("recv");
                    sk.send(&buf[..rcv]).expect("echo");
                }
            });

            c1.join().expect("join thread");
            c2.join().expect("join echo thread");
            rx.recv().expect("get rtts")
        }
    )
}

unix_bench!(unix_blocking, Blocking);
unix_bench!(unix_nonblocking, Nonblocking);

fn main() {
    let matches = clap::App::new("IPC Latency Benchmark")
        .version("0.1.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Benchmark of IPC Latency")
        .arg(Arg::with_name("iterations")
             .long("iterations")
             .short("i")
             .help("Specifies how many trials to run (default 100)")
             .default_value("100"))
        .get_matches();

    let trials = u32::from_str_radix(matches.value_of("iterations").unwrap(), 10).expect("iterations must be integral");

    for t in unix_nonblocking(trials)
        .iter()
        .map(|d| d.num_nanoseconds().unwrap()) {
        println!("unix_nonblk {:?}", t);
    }

    for t in unix_blocking(trials)
        .iter()
        .map(|d| d.num_nanoseconds().unwrap()) {
        println!("unix_blk {:?}", t);
    }

    if cfg!(target_os = "linux") {
        for t in netlink_nonblocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap()) {
            println!("nl_nonblk {:?}", t);
        }

        for t in netlink_blocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap()) {
            println!("nl_blk {:?}", t);
        }
    }

    if cfg!(target_os = "linux") {
        for t in kp_nonblocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap()) {
            println!("kp_nonblk {:?}", t);
        }

        for t in kp_blocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap()) {
            println!("kp_blk {:?}", t);
        }
    }
}
