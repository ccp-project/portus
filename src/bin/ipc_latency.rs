use clap::Arg;
use portus::ipc::{Backend, BackendSender, Blocking, Ipc, Nonblocking};
use std::convert::TryInto;
use std::sync::{atomic, Arc};
use std::thread;
use std::vec::Vec;
use time::Duration;

#[macro_use]
extern crate clap;

#[derive(Debug)]
struct TimeMsg(time::OffsetDateTime);

use std::io::prelude::*;
impl portus::serialize::AsRawMsg for TimeMsg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (0xff, portus::serialize::HDR_LENGTH + 16, 0)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> portus::Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> portus::Result<()> {
        let msg = self.0.unix_timestamp_nanos().to_le_bytes();
        w.write_all(&msg[..])?;
        Ok(())
    }

    fn from_raw_msg(msg: portus::serialize::RawMsg) -> portus::Result<Self> {
        let b = msg.get_bytes()?;
        let ts = i128::from_le_bytes((&b[0..16]).try_into().unwrap());
        Ok(TimeMsg(time::OffsetDateTime::from_unix_timestamp_nanos(
            ts,
        )?))
    }
}

use portus::serialize::AsRawMsg;
use std::sync::mpsc;
fn bench<T: Ipc>(b: BackendSender<T>, mut l: Backend<T>, iter: u32) -> Vec<Duration> {
    (0..iter)
        .map(|_| {
            let then = time::OffsetDateTime::now_utc();
            let msg = portus::serialize::serialize(&TimeMsg(then)).expect("serialize");
            b.send_msg(&msg[..]).expect("send ts");
            if let (portus::serialize::Msg::Other(raw), _addr) = l.next().expect("receive echo") {
                let then = TimeMsg::from_raw_msg(raw).expect("get time from raw");
                time::OffsetDateTime::now_utc() - then.0
            } else {
                panic!("wrong type");
            }
        })
        .collect()
}

macro_rules! netlink_bench {
    ($name: ident, $mode: ident) => {
        #[cfg(target_os = "linux")] // netlink is linux-only
        fn $name(iter: u32) -> Vec<Duration> {
            use std::process::Command;
            Command::new("sudo")
                .arg("rmmod")
                .arg("nltest")
                .output()
                .expect("rmmod failed");

            // make clean
            let make_clean = Command::new("make")
                .arg("clean")
                .current_dir("./src/ipc/test-nl-kernel")
                .output()
                .expect("make failed to start");
            assert!(make_clean.status.success());

            // compile kernel module
            let make = Command::new("make")
                .current_dir("./src/ipc/test-nl-kernel")
                .output()
                .expect("make failed to start");
            assert!(make.status.success());

            // load kernel module
            let insmod = Command::new("sudo")
                .arg("insmod")
                .arg("./src/ipc/test-nl-kernel/nltest.ko")
                .output()
                .expect("insmod failed");
            assert!(insmod.status.success());

            let (tx, rx) = mpsc::channel::<Vec<Duration>>();

            // listen
            let c1 = thread::spawn(move || {
                let mut buf = [0u8; 1024];
                let mut nl = portus::ipc::netlink::Socket::<$mode>::new()
                    .map(|sk| {
                        Backend::new(sk, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..])
                    })
                    .expect("nl ipc initialization");
                let sender = nl.sender(());
                let res = (0..iter)
                    .map(|_| {
                        let portus_send_time = time::OffsetDateTime::now_utc();
                        let msg = portus::serialize::serialize(&TimeMsg(portus_send_time))
                            .expect("serialize");
                        sender.send_msg(&msg[..]).expect("send ts");
                        if let (portus::serialize::Msg::Other(raw), _addr) =
                            nl.next().expect("recv echo")
                        {
                            let then = TimeMsg::from_raw_msg(raw).expect("get time from raw");
                            assert_eq!(then.0, portus_send_time);
                            time::OffsetDateTime::now_utc() - then.0
                        } else {
                            panic!("wrong type");
                        }
                    })
                    .collect();
                tx.send(res).expect("report rtts");
            });

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
                let mut receive_buf = [0u8; 1024];
                let kp = portus::ipc::kp::Socket::<$mode>::new()
                    .map(|sk| {
                        Backend::new(
                            sk,
                            Arc::new(atomic::AtomicBool::new(true)),
                            &mut receive_buf[..],
                        )
                    })
                    .expect("kp ipc initialization");
                tx.send(bench(kp.sender(()), kp, iter))
                    .expect("report rtts");
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
                let mut receive_buf = [0u8; 1024];
                let unix = portus::ipc::unix::Socket::<$mode>::new("bench_rx")
                    .map(|sk| {
                        Backend::new(
                            sk,
                            Arc::new(atomic::AtomicBool::new(true)),
                            &mut receive_buf[..],
                        )
                    })
                    .expect("unix ipc initialization");
                ready_rx.recv().expect("sync");
                tx.send(bench(
                    unix.sender(std::path::PathBuf::from("/tmp/ccp/bench_tx")),
                    unix,
                    iter,
                ))
                .expect("report rtts");
            });

            // echo-er
            let c2 = thread::spawn(move || {
                let sk = portus::ipc::unix::Socket::<Blocking>::new("bench_tx").expect("sk init");
                let mut buf = [0u8; 1024];
                ready_tx.send(true).expect("sync");
                for _ in 0..iter {
                    let (rcv, addr) = sk.recv(&mut buf[..]).expect("recv");
                    sk.send(&buf[..rcv], &addr).expect("echo");
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
    for t in netlink_nonblocking(trials) {
        println!("nl nonblk {:?} 0 0", t.whole_nanoseconds());
    }

    for t in netlink_blocking(trials) {
        println!("nl blk {:?} 0 0", t.whole_nanoseconds());
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
                .short('i')
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
            .map(|d| d.whole_nanoseconds())
        {
            println!("unix nonblk {:?} 0 0", t);
        }

        for t in unix_blocking(trials).iter().map(|d| d.whole_nanoseconds()) {
            println!("unix blk {:?} 0 0", t);
        }
    }

    if imps.contains(&IpcType::Nl) {
        nl_exp(trials);
    }

    if imps.contains(&IpcType::Kp) && cfg!(target_os = "linux") {
        for t in kp_nonblocking(trials).iter().map(|d| d.whole_nanoseconds()) {
            println!("kp nonblk {:?} 0 0", t);
        }

        for t in kp_blocking(trials).iter().map(|d| d.whole_nanoseconds()) {
            println!("kp blk {:?} 0 0", t);
        }
    }
}
