use std::thread;
use std::vec::Vec;

extern crate bytes;
#[macro_use]
extern crate clap;
extern crate portus;
extern crate time;

use bytes::{ByteOrder, LittleEndian};
use clap::Arg;
use portus::ipc::{Backend, BackendSender, Blocking, Ipc, Nonblocking};
use std::sync::{atomic, Arc};
use time::Duration;

#[derive(Debug)]
pub struct TimeMsg(time::Timespec);

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

    fn from_raw_msg(_msg: portus::serialize::RawMsg) -> portus::Result<Self> {
        unimplemented!()
    }
}
pub fn deserialize_timemsg(msg: portus::serialize::other::Msg) -> portus::Result<TimeMsg> {
    let b = msg.get_raw_bytes();
    let sec = LittleEndian::read_i64(&b[0..8]);
    let nsec = LittleEndian::read_i32(&b[8..12]);
    Ok(TimeMsg(time::Timespec::new(sec, nsec)))
}

#[derive(Debug)]
pub struct NlTimeMsg {
    kern_rt: time::Timespec,
    kern_st: time::Timespec,
}
impl portus::serialize::AsRawMsg for NlTimeMsg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (0xff - 1, portus::serialize::HDR_LENGTH + 16 + 8, 0)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> portus::Result<()> {
        let mut msg = [0u8; 24]; // one i64 and one i32
        LittleEndian::write_i64(&mut msg[0..8], self.kern_rt.sec);
        LittleEndian::write_i32(&mut msg[8..12], self.kern_rt.nsec);
        LittleEndian::write_i64(&mut msg[12..20], self.kern_st.sec);
        LittleEndian::write_i32(&mut msg[20..24], self.kern_st.nsec);
        w.write_all(&msg[..])?;
        Ok(())
    }

    fn from_raw_msg(_msg: portus::serialize::RawMsg) -> portus::Result<Self> {
        unimplemented!()
    }
}
pub fn deserialize_nltimemsg(msg: portus::serialize::other::Msg) -> portus::Result<NlTimeMsg> {
    let b = msg.get_raw_bytes();
    let up_sec = LittleEndian::read_i64(&b[0..8]);
    let up_nsec = LittleEndian::read_i32(&b[8..12]);
    let down_sec = LittleEndian::read_i64(&b[12..20]);
    let down_nsec = LittleEndian::read_i32(&b[20..24]);
    Ok(NlTimeMsg {
        kern_rt: time::Timespec::new(up_sec, up_nsec),
        kern_st: time::Timespec::new(down_sec, down_nsec),
    })
}

use portus::ipc::SingleBackend;
use std::sync::mpsc;
fn bench<T: Ipc>(b: BackendSender<T>, mut l: SingleBackend<T>, iter: u32) -> Vec<Duration> {
    (0..iter)
        .map(|_| {
            let then = time::get_time();
            let msg = portus::serialize::serialize(&TimeMsg(then)).expect("serialize");
            b.send_msg(&msg[..]).expect("send ts");
            if let portus::serialize::Msg::Other(raw) = l.next().expect("receive echo") {
                let then = deserialize_timemsg(raw).expect("get time from raw");
                time::get_time() - then.0
            } else {
                panic!("wrong type");
            }
        })
        .collect()
}

struct NlDuration(Duration, Duration, Duration);
macro_rules! netlink_bench {
    ($name: ident, $mode: ident) => {
        #[cfg(target_os = "linux")] // netlink is linux-only
        fn $name(iter: u32) -> Vec<NlDuration> {
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

            let (tx, rx) = mpsc::channel::<Vec<NlDuration>>();

            // listen
            let c1 = thread::spawn(move || {
                let mut nl = portus::ipc::netlink::Socket::<$mode>::new()
                    .map(|sk| SingleBackend::new(sk, Arc::new(atomic::AtomicBool::new(true))))
                    .expect("nl ipc initialization");
                tx.send(vec![]).expect("ok to insmod");
                nl.next().expect("receive echo");
                let sender = nl.sender();
                let res = (0..iter)
                    .map(|_| {
                        let portus_send_time = time::get_time();
                        let msg = portus::serialize::serialize(&TimeMsg(portus_send_time))
                            .expect("serialize");

                        sender.send_msg(&msg[..]).expect("send ts");
                        if let portus::serialize::Msg::Other(raw) = nl.next().expect("recv echo") {
                            let portus_rt = time::get_time();
                            let kern_recv_msg =
                                deserialize_nltimemsg(raw).expect("get time from raw");
                            return NlDuration(
                                portus_rt - portus_send_time,
                                kern_recv_msg.kern_rt - portus_send_time,
                                portus_rt - kern_recv_msg.kern_st,
                            );
                        } else {
                            panic!("wrong type");
                        };
                    })
                    .collect();
                tx.send(res).expect("report rtts");
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
        fn $name(_: u32) -> Vec<NlDuration> {
            vec![]
        }
    };
}

netlink_bench!(netlink_blocking, Blocking);
netlink_bench!(netlink_nonblocking, Nonblocking);

macro_rules! kp_bench {
    ($name: ident, $mode: ident) => {
        #[cfg(target_os = "linux")] // kp is linux-only
        fn $name(iter: u32) -> Vec<Duration> {
            use std::process::Command;
            let (tx, rx) = mpsc::channel::<Vec<Duration>>();

            Command::new("sudo")
                .arg("./ccp_kernel_unload")
                .current_dir("./src/ipc/test-char-dev/ccp-kernel")
                .output()
                .expect("unload failed");

            // make clean
            Command::new("make")
                .arg("clean")
                .current_dir("./src/ipc/test-char-dev/ccp-kernel")
                .output()
                .expect("make failed to start");

            // compile kernel module
            Command::new("make")
                .arg("ONE_PIPE=y")
                .current_dir("./src/ipc/test-char-dev/ccp-kernel")
                .output()
                .expect("make failed to start");

            Command::new("sudo")
                .arg("./ccp_kernel_load")
                .arg("ipc=1")
                .current_dir("./src/ipc/test-char-dev/ccp-kernel")
                .output()
                .expect("load failed");

            let c1 = thread::spawn(move || {
                let kp = portus::ipc::kp::Socket::<$mode>::new()
                    .map(|sk| SingleBackend::new(sk, Arc::new(atomic::AtomicBool::new(true))))
                    .expect("kp ipc initialization");
                tx.send(bench(kp.sender(), kp, iter)).expect("report rtts");
            });

            c1.join().expect("join kp thread");
            Command::new("sudo")
                .arg("./ccp_kernel_unload")
                .current_dir("./src/ipc/test-char-dev/ccp-kernel")
                .output()
                .expect("unload failed");
            rx.recv().expect("get rtts")
        }

        #[cfg(not(target_os = "linux"))] // kp is linux-only
        fn $name(_: u32) -> Vec<Duration> {
            vec![]
        }
    };
}

kp_bench!(kp_blocking, Blocking);
kp_bench!(kp_nonblocking, Nonblocking);

macro_rules! unix_bench {
    ($name: ident, $mode: ident) => {
        fn $name(iter: u32) -> Vec<Duration> {
            let (tx, rx) = mpsc::channel::<Vec<Duration>>();
            let (ready_tx, ready_rx) = mpsc::channel::<bool>();

            // listen
            let c1 = thread::spawn(move || {
                let unix = portus::ipc::unix::Socket::<$mode>::new(1, "in", "out")
                    .map(|sk| SingleBackend::new(sk, Arc::new(atomic::AtomicBool::new(true))))
                    .expect("unix ipc initialization");
                ready_rx.recv().expect("sync");
                tx.send(bench(unix.sender(), unix, iter))
                    .expect("report rtts");
            });

            // echo-er
            let c2 = thread::spawn(move || {
                let sk =
                    portus::ipc::unix::Socket::<Blocking>::new(1, "out", "in").expect("sk init");
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
    };
}

unix_bench!(unix_blocking, Blocking);
unix_bench!(unix_nonblocking, Nonblocking);

arg_enum! {
    #[derive(PartialEq, Debug)]
    pub enum IpcType {
        Nl,
        Unix,
        Kp,
    }
}

#[cfg(target_os = "linux")]
fn nl_exp(trials: u32) {
    for t in netlink_nonblocking(trials).iter().map(|d| {
        (
            d.0.num_nanoseconds().unwrap(),
            d.1.num_nanoseconds().unwrap(),
            d.2.num_nanoseconds().unwrap(),
        )
    }) {
        println!("nl nonblk {:?} {:?} {:?}", t.0, t.1, t.2);
    }

    for t in netlink_blocking(trials).iter().map(|d| {
        (
            d.0.num_nanoseconds().unwrap(),
            d.1.num_nanoseconds().unwrap(),
            d.2.num_nanoseconds().unwrap(),
        )
    }) {
        println!("nl blk {:?} {:?} {:?}", t.0, t.1, t.2);
    }
}

#[cfg(not(target_os = "linux"))]
fn nl_exp(trials: u32) {
    netlink_blocking(trials);
    netlink_nonblocking(trials);
}

fn main() {
    let matches = clap::App::new("IPC Latency Benchmark")
        .version("0.2.0")
        .author("Akshay Narayan <akshayn@mit.edu>")
        .about("Benchmark of IPC Latency")
        .arg(
            Arg::with_name("iterations")
                .long("iterations")
                .short("i")
                .help("Specifies how many trials to run (default 100)")
                .default_value("100"),
        )
        .arg(
            Arg::with_name("implementation")
                .long("impl")
                .help("Specifies the type of ipc being benchmarked")
                .possible_values(&IpcType::variants())
                .case_insensitive(true)
                .multiple(true)
                .default_value("nl"),
        )
        .get_matches();

    let trials = u32::from_str_radix(matches.value_of("iterations").unwrap(), 10)
        .expect("iterations must be integral");

    let imps = values_t!(matches.values_of("implementation"), IpcType).unwrap();

    println!("Impl Mode Rtt To From");
    if imps.contains(&IpcType::Unix) {
        for t in unix_nonblocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap())
        {
            println!("unix nonblk {:?} 0 0", t);
        }

        for t in unix_blocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap())
        {
            println!("unix blk {:?} 0 0", t);
        }
    }

    if imps.contains(&IpcType::Nl) {
        nl_exp(trials);
    }

    if imps.contains(&IpcType::Kp) && cfg!(target_os = "linux") {
        for t in kp_nonblocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap())
        {
            println!("kp nonblk {:?} 0 0", t);
        }

        for t in kp_blocking(trials)
            .iter()
            .map(|d| d.num_nanoseconds().unwrap())
        {
            println!("kp blk {:?} 0 0", t);
        }
    }
}
