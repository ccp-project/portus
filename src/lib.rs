#![feature(box_patterns)]
#![cfg_attr(feature = "bench", feature(test))]

extern crate bytes;
extern crate clap;
extern crate libc;
extern crate nix;
#[macro_use]
extern crate nom;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

pub mod ipc;
pub mod lang;
#[macro_use]
pub mod pattern;
pub mod serialize;

use std::collections::HashMap;

use ipc::Ipc;
use ipc::Backend;
use serialize::Msg;

#[derive(Debug)]
pub struct Error(pub String);

impl From<String> for Error {
    fn from(e: String) -> Error {
        Error(e)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<lang::Error> for Error {
    fn from(e: lang::Error) -> Error {
        Error(format!("lang err: {:?}", e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

use std::rc::Rc;
pub struct Datapath<T: Ipc>(Rc<Backend<T>>);

use lang::{Reg, Scope};
impl<T: Ipc> Datapath<T> {
    /// Algorithm implementations use send_pattern() to control the datapath's behavior by
    /// calling send_pattern() with:
    /// 1. An initialized backend b.
    /// 2. The flow's sock_id. IPC implementations supporting addressing (e.g. Unix sockets, which can
    /// communicate with many applications using UDP datapaths)  MUST make the address be sock_id
    /// 3. The control pattern prog to install. Implementations can create patterns using make_pattern!.
    /// send_pattern() will return quickly with a Result indicating whether the send was successful.
    pub fn send_pattern(&self, sock_id: u32, prog: pattern::Pattern) -> Result<()> {
        let msg = serialize::pattern::Msg {
            sid: sock_id,
            num_events: prog.len() as u32,
            pattern: prog,
        };

        let buf = serialize::serialize(msg)?;
        self.0.send_msg(&buf[..])?;
        Ok(())
    }

    pub fn install_measurement(&self, sock_id: u32, src: &[u8]) -> Result<Scope> {
        let (bin, sc) = lang::compile(src)?;
        let msg = serialize::install_fold::Msg {
            sid: sock_id,
            num_instrs: bin.0.len() as u32,
            instrs: bin,
        };

        let buf = serialize::serialize(msg)?;
        self.0.send_msg(&buf[..])?;
        Ok(sc)
    }
}

#[cfg(all(target_os = "linux"))]
pub fn ipc_valid(v: String) -> std::result::Result<(), String> {
    match v.as_str() {
        "netlink" | "unix" => Ok(()),
        _ => Err(String::from(
            format!("ipc must be one of (netlink|unix): {:?}", v),
        )),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn ipc_valid(v: String) -> std::result::Result<(), String> {
    match v.as_str() {
        "unix" => Ok(()),
        _ => Err(String::from(format!("ipc must be one of (unix): {:?}", v))),
    }
}

pub struct Measurement {
    fields: Vec<u64>,
}

impl Measurement {
    pub fn get_field(&self, field: &String, sc: &Scope) -> Option<u64> {
        sc.get(field).and_then(|r| match r {
            &Reg::Perm(idx, _) => {
                if idx as usize >= self.fields.len() {
                    println!("Didn't find fields in the message! {:?}", self.fields);
                    return None;
                }

                Some(self.fields[idx as usize])
            },
            _ => {
                println!("Didn't find fields in the message! {:?}", self.fields);
                None
            },
        })
    }
}

pub trait CongAlg<T: Ipc> {
    type Config: Clone;
    fn name() -> String;
    fn create(control: Datapath<T>, cfg: Config<T, Self>, info: DatapathInfo) -> Self;
    fn measurement(&mut self, sock_id: u32, m: Measurement);
}

pub struct Config<I, U: ?Sized>
where
    I: Ipc,
    U: CongAlg<I>,
{
    pub logger: Option<slog::Logger>,
    pub config: U::Config,
}

// Cannot #[derive(Clone)] on Config because the compiler does not realize
// we are not using I or U, only U::Config.
// https://github.com/rust-lang/rust/issues/26925
impl<I, U> Clone for Config<I, U>
where
    I: Ipc,
    U: CongAlg<I>,
{
    fn clone(&self) -> Self {
        Config {
            logger: self.logger.clone(),
            config: self.config.clone(),
        }
    }
}

#[derive(Copy, Clone)]
pub struct DatapathInfo {
    pub sock_id: u32,
    pub init_cwnd: u32,
    pub mss: u32,
    pub src_ip: u32,
    pub src_port: u32,
    pub dst_ip: u32,
    pub dst_port: u32,
}

/// Main CCP execution loop for a single congestion control algorithm.
/// Blocks "forever".
/// In this use case, an algorithm implementation is a binary which
/// 1. Provides a U corresponding a to a single flow's congestion state.
/// 2. Initializes an ipc backend (depending on datapath)
/// 3. Calls start(), passing the backend b and a Config with optional
/// logger and command line argument structure.
///
/// start():
/// 1. listens for messages from the datapath
/// 2. call the appropriate message in U: impl CongAlg
///
/// start() will never return (-> !). It will panic if:
/// 1. It receives a pattern or install_fold control message (only a datapath should receive these)
/// 2. The IPC channel fails.
pub fn start<I, U>(b: Backend<I>, cfg: Config<I, U>) -> !
where
    I: Ipc,
    U: CongAlg<I>,
{
    let mut flows = HashMap::<u32, U>::new();
    let backend = std::rc::Rc::new(b);
    for m in backend.listen().iter() {
        if let Ok(msg) = Msg::from_buf(&m[..]) {
            match msg {
                Msg::Cr(c) => {
                    if let Some(_) = flows.remove(&c.sid) {
                        cfg.logger.as_ref().map(|log| {
                            debug!(log, "re-creating already created flow"; "sid" => c.sid);
                        });
                    }

                    cfg.logger.as_ref().map(|log| {
                        debug!(log, "creating new flow"; 
                               "sid" => c.sid, 
                               "init_cwnd" => c.init_cwnd,
                               "mss"  =>  c.mss,
                               "src_ip"  =>  c.src_ip,
                               "src_port"  =>  c.src_port,
                               "dst_ip"  =>  c.dst_ip,
                               "dst_port"  =>  c.dst_port,
                        );
                    });

                    let alg = U::create(
                        Datapath(backend.clone()),
                        cfg.clone(),
                        DatapathInfo {
                            sock_id: c.sid,
                            init_cwnd: c.init_cwnd,
                            mss: c.mss,
                            src_ip: c.src_ip,
                            src_port: c.src_port,
                            dst_ip: c.dst_ip,
                            dst_port: c.dst_port,
                        },
                    );
                    flows.insert(c.sid, alg);
                }
                Msg::Ms(m) => {
                    if let Some(alg) = flows.get_mut(&m.sid) {
                        alg.measurement(m.sid, Measurement { fields: m.fields })
                    } else {
                        cfg.logger.as_ref().map(|log| {
                            debug!(log, "measurement for unknown flow"; "sid" => m.sid);
                        });
                    }
                }
                Msg::Pt(_) | Msg::Fld(_) => {
                    panic!(
                        "The start() listener should never receive a pattern \
                        or install_fold message, since it is on the CCP side."
                    )
                }
                _ => continue,
            }
        }
    }

    panic!("The IPC receive channel closed.");
}

/// Implementations of congestion control across multiple flows.
pub trait Aggregator<T: Ipc> {
    /// Aggregators define this type to keep state when binning flows into aggregates.
    type Key: From<DatapathInfo> + std::cmp::Eq + std::hash::Hash + Copy;

    /// If a new flow corresponds to an existing aggregate, replace the create() method
    /// from CongAlg with new_flow() to notify the aggregate of a new flow arrival.
    fn new_flow(&mut self, info: DatapathInfo, control: Datapath<T>);
}

/// Main CCP execution loop for an aggregate congestion control algorithm
/// Blocks "forever".
/// In this use case, an algorithm implementation is a binary which
/// 1. Corresponds to an aggregate of flows, all using the same congestion control.
/// 2. Initializes an ipc backend (depending on datapath)
/// 3. Calls start(), passing the backend b and a Config with optional
/// logger and command line argument structure.
///
/// start_aggregator():
/// 1. listens for messages from the datapath
/// 2. call the appropriate message in U: impl CongAlg + Aggregator
///
/// start_aggregator() will never return (-> !). It will panic if:
/// 1. It receives a pattern or install_fold control message (only a datapath should receive these)
/// 2. The IPC channel fails.
pub fn start_aggregator<I, U>(b: Backend<I>, cfg: Config<I, U>) -> !
where
    I: Ipc,
    U: CongAlg<I> + Aggregator<I>,
{
    let mut aggregates = HashMap::<U::Key, U>::new();
    let mut flows = HashMap::<u32, U::Key>::new();
    let backend = std::rc::Rc::new(b);
    for m in backend.listen().iter() {
        if let Ok(msg) = Msg::from_buf(&m[..]) {
            match msg {
                Msg::Cr(c) => {
                    let d = DatapathInfo {
                        sock_id: c.sid,
                        init_cwnd: c.init_cwnd,
                        mss: c.mss,
                        src_ip: c.src_ip,
                        src_port: c.src_port,
                        dst_ip: c.dst_ip,
                        dst_port: c.dst_port,
                    };

                    let k = U::Key::from(d);
                    aggregates
                        .get_mut(&k)
                        .and_then(|agg| {
                            agg.new_flow(d,
                                         Datapath(backend.clone()));
                            Some(())
                        })
                        .or_else(|| {
                            let agg = U::create(
                                Datapath(backend.clone()),
                                cfg.clone(),
                                DatapathInfo {
                                    sock_id: c.sid,
                                    init_cwnd: c.init_cwnd,
                                    mss: c.mss,
                                    src_ip: c.src_ip,
                                    src_port: c.src_port,
                                    dst_ip: c.dst_ip,
                                    dst_port: c.dst_port,
                                },
                            );

                            aggregates.insert(k, agg);
                            Some(())
                        });

                    flows.insert(d.sock_id, k);
                }
                Msg::Ms(m) => {
                    let sid = m.sid;
                    flows
                        .get(&sid)
                        .and_then(|key| {
                            aggregates.get_mut(&key).and_then(move |agg| {
                                agg.measurement(sid, Measurement { fields: m.fields });
                                Some(())
                            })
                        })
                        .or_else(|| {
                            cfg.logger.as_ref().map(|log| {
                                debug!(log, "measurement for unknown flow"; "sid" => sid);
                            })
                        });
                }
                Msg::Pt(_) | Msg::Fld(_) => {
                    panic!(
                        "The start() listener should never receive a pattern \
                        or install_fold message, since it is on the CCP side."
                    )
                }
                _ => continue,
            }
        }
    }

    panic!("The IPC receive channel closed.");
}

#[cfg(test)]
mod test;
