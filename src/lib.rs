#![feature(box_patterns)]
#![feature(test)]
#![feature(never_type)]

extern crate bytes;
extern crate clap;
extern crate libc;
extern crate nix;
#[macro_use]
extern crate nom;
extern crate time;

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

pub mod ipc;
pub mod lang;
pub mod serialize;
pub mod test_helper;
#[macro_use]
pub mod algs;

use std::collections::HashMap;

use ipc::Ipc;
use ipc::{BackendSender, BackendBuilder};
use serialize::Msg;
use std::sync::{Arc, atomic};
use std::thread;

#[derive(Clone, Debug)]
pub struct Error(pub String);

impl<T: std::error::Error + std::fmt::Display> From<T> for Error {
    fn from(e: T) -> Error {
        Error(format!("portus err: {}", e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait DatapathTrait {
    fn install(&self, src: &[u8]) -> Result<Scope>;
    fn update_field(&self, sc: &Scope, update: &[(&str, u32)]) -> Result<()>;
    fn get_sock_id(&self) -> u32;
}

pub struct Datapath<T: Ipc>{
    sock_id: u32,
    sender: BackendSender<T>,
}

use lang::{Reg, Scope};
impl<T: Ipc> DatapathTrait for Datapath<T> {
    fn get_sock_id(&self) -> u32 {
        return self.sock_id;
    }
    
    fn install(&self, src: &[u8]) -> Result<Scope> {
        let (bin, sc) = lang::compile(src)?;
        let msg = serialize::install::Msg {
            sid: self.sock_id,
            num_events: bin.events.len() as u32,
            num_instrs: bin.instrs.len() as u32,
            instrs: bin,
        };

        let buf = serialize::serialize(&msg)?;
        self.sender.send_msg(&buf[..])?;
        Ok(sc)
    }

    /// pass a Scope and (Reg name, new_value) pairs
    fn update_field(&self, sc: &Scope, update: &[(&str, u32)]) -> Result<()> {
        let fields : Vec<(Reg, u64)> = update.iter().map(
            |&(reg_name, new_value)| {
                if reg_name.starts_with("__") {
                    return Err(Error(
                        format!("Cannot update reserved field: {:?}", reg_name)
                    ));
                }

                sc.get(reg_name)
                    .ok_or_else(|| Error(
                        format!("Unknown field: {:?}", reg_name)
                    ))
                    .and_then(|reg| match *reg {
                        ref r@Reg::Control(_, _) => {
                            Ok((r.clone(), u64::from(new_value)))
                        }
                        Reg::Implicit(idx, ref t) if idx == 4 || idx == 5 => {
                            Ok((Reg::Implicit(idx, t.clone()), u64::from(new_value)))
                        }
                        _ => Err(Error(
                            format!("Cannot update field: {:?}", reg_name),
                        )),
                    })
            }
        ).collect::<Result<_>>()?;

        let msg = serialize::update_field::Msg{
            sid: self.sock_id,
            num_fields: fields.len() as u8,
            fields
        };

        let buf = serialize::serialize(&msg)?;
        self.sender.send_msg(&buf[..])?;
        Ok(())
    }
}

pub struct Report {
    fields: Vec<u64>,
}

impl Report {
    pub fn get_field(&self, field: &str, sc: &Scope) -> Option<u64> {
        sc.get(field).and_then(|r| match *r {
            Reg::Report(idx, _, _) => {
                if idx as usize >= self.fields.len() {
                    return None;
                }

                Some(self.fields[idx as usize])
            },
            _ => None,
        })
    }
}

pub trait CongAlg<T: Ipc> {
    type Config: Clone;
    fn name() -> String;
    fn create(control: Datapath<T>, cfg: Config<T, Self>, info: DatapathInfo) -> Self;
    fn on_report(&mut self, sock_id: u32, m: Report);
    fn close(&mut self) {} // default implementation does nothing (optional method)
}

pub struct Config<I, U: ?Sized>
where
    I: Ipc,
    U: CongAlg<I> + 'static,
{
    pub logger: Option<slog::Logger>,
    pub config: U::Config,
}

unsafe impl<I, U: ?Sized> Sync for Config<I, U>
where
    I: Ipc,
    U: CongAlg<I> + 'static{}


unsafe impl<I, U: ?Sized> Send for Config<I, U>
where
    I: Ipc,
    U: CongAlg<I> {}

// Cannot #[derive(Clone)] on Config because the compiler does not realize
// we are not using I or U, only U::Config.
// https://github.com/rust-lang/rust/issues/26925
impl<I, U> Clone for Config<I, U>
where
    I: Ipc,
    U: CongAlg<I> + 'static,
{
    fn clone(&self) -> Self {
        Config {
            logger: self.logger.clone(),
            config: self.config.clone(),
        }
    }
}

#[derive(Clone)]
pub struct DatapathInfo {
    pub sock_id: u32,
    pub init_cwnd: u32,
    pub mss: u32,
    pub src_ip: u32,
    pub src_port: u32,
    pub dst_ip: u32,
    pub dst_port: u32,
}

#[derive(Debug)]
pub struct CCPHandle {
    pub continue_listening: Arc<atomic::AtomicBool>,
    pub join_handle: thread::JoinHandle<Result<()>>,
}

impl CCPHandle {
    /// sets the Arc<AtomicBool> to be false, so running function can return
    pub fn kill(&self) {
       self.continue_listening.store(false, atomic::Ordering::SeqCst);
    }

    /// handles errors from the run_inner processs that was just launched
    /// TODO: join_handle.join() returns an Err instead of Ok, because
    /// some function panicked, this function should return an error
    /// with the same string from the panic.
    pub fn wait(self) -> Result<()> {
        match self.join_handle.join() {
            Ok(r) => r,
            Err(_) => Err(Error(String::from("Call to run_inner panicked"))),
        }
    }
}

/// Main execution loop of ccp for the static pipeline use case.
/// The run method blocks 'forever' - and only returns if something caused execution to fail.
/// Callers must construct a backend builder, and a config to pass into run_inner.
/// The call to run_inner should never return unless there was an error.
/// Takes a reference to a config
pub fn run<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>) -> Result<!>
where
    I: Ipc,
    U: CongAlg<I>,
{
    // call run_inner
    match run_inner(backend_builder, cfg, Arc::new(atomic::AtomicBool::new(true))) {
        Ok(_) => unreachable!(),
        Err(e) => Err(e),
    }
}

/// Spawns a ccp process, and returns a CCPHandle object.
/// The caller can call kill on the CCPHandle.
/// This causes the backend built from the backend builder to set
/// a flag to false and stop iterating.
/// Takes a config (not reference)
pub fn spawn<I, U>(backend_builder: BackendBuilder<I>, cfg: Config<I, U>) -> CCPHandle
where
    I: Ipc,
    U: CongAlg<I>,
{
    let stop_signal = Arc::new(atomic::AtomicBool::new(true));
    CCPHandle {
        continue_listening: stop_signal.clone(),
        join_handle: thread::spawn(move || {
            run_inner(backend_builder, &cfg, stop_signal.clone())
        }),
    }
}

/// Main execution inner loop of ccp.
/// Blocks "forever", or until the iterator stops iterating.
/// In this use case, an algorithm implementation is a binary which
/// 1. Initializes an ipc backendbuilder (depending on the datapath).
/// 2. Calls `run()`, or `spawn() `passing the `BackendBuilder b` and a `Config` with optional
/// logger and command line argument structure.
/// Run() or spawn() create arc<AtomicBool> objects,
/// which are passed into run_inner to build the backend, so spawn() can create a CCPHandle that references this
/// boolean to kill the thread.
///
/// `run_inner()`:
/// 1. listens for messages from the datapath
/// 2. call the appropriate message in `U: impl CongAlg`
/// The function can return for two reasons: an error, or the iterator returned None.
/// The latter should only happen for spawn(), and not for run().
/// It returns any error, either from:
/// 1. the IPC channel failing
/// 2. Receiving an install control message (only the datapath should receive these).
fn run_inner<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>, continue_listening: Arc<atomic::AtomicBool>)  -> Result<()>
where
    I: Ipc,
    U: CongAlg<I>,
{
    let mut receive_buf = [0u8; 1024];
    let mut  b = backend_builder.build(continue_listening.clone(), &mut receive_buf[..]);
    let mut flows = HashMap::<u32, U>::new();
    let backend = b.sender();
    while let Some(msg) = b.next() {
        match msg {
            Msg::Cr(c) => {
                if flows.remove(&c.sid).is_some() {
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
                    Datapath{
                        sock_id: c.sid, 
                        sender: backend.clone()
                    },
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
                if flows.contains_key(&m.sid) {
                    if m.num_fields == 0 {
                        let mut alg = flows.remove(&m.sid).unwrap();
                        alg.close();
                    } else {
                        let alg = flows.get_mut(&m.sid).unwrap();
                        alg.on_report(m.sid, Report { fields: m.fields })
                    }
                } else {
                    cfg.logger.as_ref().map(|log| {
                        debug!(log, "measurement for unknown flow"; "sid" => m.sid);
                    });
                }
            }
            Msg::Ins(_) => {
                unimplemented!()
                //return Err(Error(String::from("The start() listener should never receive an install \
                //    message, since it is on the CCP side.")));
            }
            _ => continue,
        }
    }
    // if the thread has been killed, return that as error
    if !continue_listening.load(atomic::Ordering::SeqCst) {
        Ok(())
    } else {
        Err(Error(String::from("The IPC channel has closed.")))
    }
}
#[cfg(test)]
mod test;
