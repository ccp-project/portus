//! Welcome to CCP.
//!
//! This crate, portus, implements a CCP. This includes:
//! 1. An interface definition for external types wishing to implement congestion control
//!    algorithms (`CongAlg`).
//! 2. A [compiler](lang/index.html) for datapath programs.
//! 3. An IPC and serialization [layer](ipc/index.html) for communicating with libccp-compliant datapaths.
//!
//! The entry points into portus are [run](./fn.run.html) and [spawn](./fn.spawn.html), which start
//! the CCP algorithm runtime. This runtime listens for datapath messages and dispatches calls to
//! the appropriate congestion control methods.
//!
//! Example
//! =======
//! 
//! The following congestion control algorithm sets the congestion window to `42`, and prints the
//! minimum RTT observed over 42 millisecond intervals.
//!
//! ```
//! extern crate portus;
//! use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
//! use portus::ipc::Ipc;
//! use portus::lang::Scope;
//! use portus::lang::Bin;
//! struct MyCongestionControlAlgorithm(Scope);
//! #[derive(Clone)]
//! struct MyEmptyConfig;
//!
//! impl<T: Ipc> CongAlg<T> for MyCongestionControlAlgorithm {
//!     type Config = MyEmptyConfig;
//!     fn name() -> String {
//!         String::from("My congestion control algorithm")
//!     }
//!     fn init_programs() -> Vec<(String, String)> {
//!         vec![
//!             (String::from("MyProgram"), String::from("
//!                 (def (Report
//!                     (volatile minrtt +infinity)
//!                 ))
//!                 (when true
//!                     (:= Report.minrtt (min Report.minrtt Flow.rtt_sample_us))
//!                 )
//!                 (when (> Micros 42000)
//!                     (report)
//!                     (reset)
//!                 )
//!             ")),
//!         ]
//!     }
//!     fn create(mut control: Datapath<T>, cfg: Config<T, Self>, info: DatapathInfo) -> Self {
//!         let sc = control.set_program(String::from("MyProgram"), None).unwrap();
//!         MyCongestionControlAlgorithm(sc)
//!     }
//!     fn on_report(&mut self, sock_id: u32, m: Report) {
//!         println!("minrtt: {:?}", m.get_field("Report.minrtt", &self.0).unwrap());
//!     }
//! }
//! ```

#![feature(box_patterns)]
#![feature(test)]
#![feature(never_type)]
#![feature(integer_atomics)]
#![feature(extern_prelude)]

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

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate rmp;
extern crate rmp_serde as rmps;

pub mod ipc;
pub mod lang;
pub mod serialize;
pub mod test_helper;
#[macro_use]
pub mod algs;
mod errors;
pub use errors::*;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use ipc::Ipc;
use ipc::{BackendSender, BackendBuilder};
use serialize::Msg;
use serialize::summary::Summary;
use serialize::allocation::Allocation;
use std::sync::{Arc, atomic};
use std::thread;
use lang::{Reg, Scope, Bin};

/// CCP custom `Result` type, using `Error` as the `Err` type.
pub type Result<T> = std::result::Result<T, Error>;

/// A collection of methods to interact with the datapath.
pub trait DatapathTrait {
    fn get_sock_id(&self) -> u32;
    /// Tell datapath to use a preinstalled program.
    fn set_program(&self, program_name: String, fields: Option<&[(&str, u32)]>) -> Result<Scope>;
    /// Update the value of a register in an already-installed fold function.
    fn update_field(&self, sc: &Scope, update: &[(&str, u32)]) -> Result<()>;
}

/// A collection of methods to interact with the datapath.
pub struct Datapath<T: Ipc>{
    sock_id: u32,
    sender: BackendSender<T>,
    programs: Rc<HashMap<String, Scope>>,
}

impl<T: Ipc> DatapathTrait for Datapath<T> {
    fn get_sock_id(&self) -> u32 {
        return self.sock_id;
    }

    fn set_program(&self, program_name: String, fields: Option<&[(&str, u32)]>) -> Result<Scope> {
        // if the program with this key exists, return it; otherwise return nothing
        match self.programs.get(&program_name) {
            Some(sc) => {
                // apply optional updates to values of registers in this scope
                let fields : Vec<(Reg, u64)> = fields.unwrap_or_else(|| &[]).iter().map(
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
                                Reg::Control(idx, ref t) => {
                                    Ok((Reg::Control(idx, t.clone()), u64::from(new_value)))
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
                let msg = serialize::changeprog::Msg {
                    sid: self.sock_id,
                    program_uid: sc.program_uid,
                    num_fields: fields.len() as u32,
                    fields
                };
                let buf = serialize::serialize(&msg)?;
                self.sender.send_msg(&buf[..])?;
                Ok(sc.clone())
            },
            _ => Err(Error(
                format!("Map does not contain datapath program with key: {:?}", program_name),
            )),
        }
    }


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
                        Reg::Control(idx, ref t) => {
                            Ok((Reg::Control(idx, t.clone()), u64::from(new_value)))
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

/// Defines a `slog::Logger` to use for (optional) logging 
/// and a custom `CongAlg::Config` to pass into algorithms as new flows
/// are created.
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

#[derive(Copy, Clone)]
/// The set of information passed by the datapath to CCP
/// when a connection starts. It includes a unique 5-tuple (CCP socket id + source and destination
/// IP and port), the initial congestion window (`init_cwnd`), and flow MSS.
pub struct DatapathInfo {
    pub sock_id: u32,
    pub init_cwnd: u32,
    pub mss: u32,
    pub src_ip: u32,
    pub src_port: u32,
    pub dst_ip: u32,
    pub dst_port: u32,
}

/// Contains the values of the pre-defined Report struct from the fold function.
/// Use `get_field` to query its values using the names defined in the fold function.
pub struct Report {
    pub program_uid: u32, 
        fields: Vec<u64>,
}

impl Report {
    /// Uses the `Scope` returned by `lang::compile` (or `install`) to query 
    /// the `Report` for its values.
    pub fn get_field(&self, field: &str, sc: &Scope) -> Result<u64> {
        match sc.get(field) {
            Some(r) => {
                match *r {
                    Reg::Report(idx, _, _) => {
                        if idx as usize >= self.fields.len() {
                            Err(Error::from(InvalidReportError))
                        } else {
                            Ok(self.fields[idx as usize])
                        }
                    },
                    _ => Err(Error::from(InvalidRegTypeError)),
                }
            },
            None => Err(Error::from(FieldNotFoundError)),
        }
    }
}

/// Implement this trait to define a CCP congestion control algorithm.
pub trait CongAlg<T: Ipc> {
    /// Implementors use `Config` to define custion configuration parameters.
    type Config: Clone;
    fn name() -> String;
    /// This function is expected to return all datapath programs the congestion control algorithm
    /// may want to use at any point during its execution. It is called only once, when Portus initializes
    /// ([`portus::run`](./fn.run.html) or [`portus::spawn`](./fn.spawn.html)).
    ///
    /// It should return a vector of string tuples, where the first string in each tuple is a unique name
    /// identifying the program, and the second string is the code for the program itself.
    ///
    /// Portus will panic if any of the datapath programs do not compile.
    ///
    /// For example,
    /// ```
    /// vec![(String::from("prog1"), String::from("...(program)...")),
    ///      (String::from("prog2"), String::from("...(program)..."))
    /// ];
    /// ```
    fn init_programs() -> Vec<(String, String)>;
    fn create(control: Datapath<T>, cfg: Config<T, Self>, info: DatapathInfo) -> Self;
    fn on_report(&mut self, sock_id: u32, m: Report);
    fn close(&mut self) {} // default implementation does nothing (optional method)
}

/// Implement this trait (and [`CongAlg`](./trait.CongAlg.html)) to define an algorithm that performs aggregate congestion control across multiple flows.
/// An instance of a struct implementing this trait represents an aggregate bundle of flows.
///
/// The internal [`Key`](./trait.Aggregator.html#associatedtype.Key) type determines which flows belong to this bundle. 
/// 1. [`CongAlg::create`](./trait.CongAlg.html#tymethod.create) is called when the first flow matching this key is started.
/// 2. [`new_flow`](./trait.Aggregator.html#tymethod.new_flow) is called for each additional flow that starts and joins the bundle.
/// 3. [`close_one`](./trait.Aggregator.html#tymethod.close_one) is called each time a flow finishes and leaves the bundles. 
/// 4. [`CongAlg::close`](./trait.CongAlg.html#method.close) is called when there are no longer any flows belonging to this bundle.
/// Immediately after this call the struct will be destroyed. 
pub trait Aggregator<T: Ipc> {
    /// Aggregators provide this type to define how flows are binned into aggregates.
    /// This key must implement the equality, hash, debug, and copy traits,
    /// and is as a function of the corresponding flow's DatapathInfo struct.
    type Key: From<DatapathInfo> + std::cmp::Eq + std::hash::Hash + std::fmt::Debug + Copy + Default;

    /// If a new flow corresponds to an existing aggregate, replace the create() method
    /// from CongAlg with new_flow() to notify the aggregate of a new flow arrival.
    fn new_flow(&mut self, control: Datapath<T>, info: DatapathInfo);

    /// Called when a flow belonging to this aggregate ends.
    fn close_one(&mut self, sock_id: u32);

}

/// Add this trait to a struct that implements CongAlg and Aggregator to define an algorithm that
/// can participate in cluster congestion control by sending aggregate summaries about network
/// conditions to a master, and obeying its corresponding rate allocation orders.
pub trait Slave {
    /// Algorithms are expected to maintain statistics internally from Reports and then use this
    /// function to combine those statistics into a single summary that will be sent to the
    /// controller.
    fn create_summary(&mut self) -> Summary;

    /// The number of microseconds to wait until sending the next summary. This function is called
    /// first when the aggregate is created and a corresponding timer is set. Once that timer
    /// expires, create_summary is called, and the summary message is sent to the master. Then this
    /// function is called again to reset the timer, and so on. 
    fn next_summary_time(&mut self) -> u32;

    /// This function is used in place of CongAlg::on_report() for cluster congestion control
    /// algorithms. The signature is the same, except that it returns a boolean -- if true, a
    /// Summary message will be sent immediately, otherwise it is handled the same as
    /// CongAlg::on_report().
    ///
    /// Algorithms should implement Report handling logic here, and then define
    /// CongAlg::on_report() as a single line that calls Slave::on_report() and ignores the return
    /// value.
    ///
    /// This adds the ability for algorithms to send a summary immediately in response to
    /// a Report, even if the timer has not expired yet.  For example, an algorithm may want to
    /// notify the master immediately in the event of a drop.
    fn on_report(&mut self, sock_id: u32, r: Report) -> bool;

    /// Called when an Allocation message is received from the controller. Algorithms must adhere
    /// to this rate in aggregate, but are free to decide how to allocate that rate among individual flows. 
    fn on_allocation(&mut self, a: Allocation);
}

#[derive(Debug)]
/// A handle to manage running instances of the CCP execution loop.
pub struct CCPHandle {
    pub continue_listening: Arc<atomic::AtomicBool>,
    pub join_handle: thread::JoinHandle<Result<()>>,
}

impl CCPHandle {
    /// Instruct the execution loop to exit.
    pub fn kill(&self) {
       self.continue_listening.store(false, atomic::Ordering::SeqCst);
    }

    // TODO: join_handle.join() returns an Err instead of Ok, because
    // some function panicked, this function should return an error
    // with the same string from the panic.
    /// Collect the error from the thread running the CCP execution loop
    /// once it exits.
    pub fn wait(self) -> Result<()> {
        match self.join_handle.join() {
            Ok(r) => r,
            Err(_) => Err(Error(String::from("Call to run_inner panicked"))),
        }
    }
}

/// Main execution loop of CCP for the static pipeline use case.
/// The `run` method blocks 'forever'; it only returns in two cases:
/// 1. The IPC socket is closed.
/// 2. An invalid message is received.
///
/// Callers must construct a `BackendBuilder` and a `Config`.
/// Algorithm implementations should
/// 1. Initializes an ipc backendbuilder (depending on the datapath).
/// 2. Calls `run()`, or `spawn() `passing the `BackendBuilder b` and a `Config` with optional
/// logger and command line argument structure.
/// Run() or spawn() create arc<AtomicBool> objects,
/// which are passed into run_inner to build the backend, so spawn() can create a CCPHandle that references this
/// boolean to kill the thread.
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

/// Aggregate congestion control version of [`run`](./fn.run.html).
/// 
/// Same as [`run`](./fn.run.html), except `U` must also implement the [`Aggregator<I>`](./trait.Aggregator.html) trait in addition to [`CongAlg<I>`](./trait.CongAlg.html).
pub fn run_aggregator<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>) -> Result<!>
where
    I: Ipc,
    U: CongAlg<I> + Aggregator<I>,
{
    match run_aggregator_inner(backend_builder, cfg, Arc::new(atomic::AtomicBool::new(true))) {
        Ok(_) => unreachable!(),
        Err(e) => Err(e),
    }
}

/// Cluster congestion control version of [`run_aggregator`](./fn.run_aggregator.html).
///
/// Same as run_aggregator, except `U` must additionally implement the [`Slave<I>`](./trait.Slave.html) trait, 
/// and caller must provide addresses for connecting to the controller via a non-blocking UDP socket.
/// It will periodically send Summary messages to the
/// controller, who will respond with an Allocation message specifying the aggregate rate for this
/// machine. The algorithm can internally decide how it wishes to schedule flows within that
/// allocation.
pub fn run_aggregator_with_remote<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>, local_addr : String, controller_addr: String) -> Result<!>
where 
    I: Ipc,
    U: CongAlg<I> + Aggregator<I> + Slave,
{
    match run_aggregator_with_remote_inner(backend_builder, cfg, Arc::new(atomic::AtomicBool::new(true)), local_addr, controller_addr) {
        Ok(_) => unreachable!(),
        Err(e) => Err(e),
    }
}

/// Spawn a thread which will perform the CCP execution loop. Returns
/// a `CCPHandle`, which the caller can use to cause the execution loop
/// to stop.
/// The `run` method blocks 'forever'; it only returns in three cases:
/// 1. The IPC socket is closed.
/// 2. An invalid message is received.
/// 3. The caller calls `CCPHandle::kill()`
///
/// See [`run`](./fn.run.html) for more information.
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

/// Aggregate congestion control version of [`spawn`](./fn.spawn.html).
///
/// Same as [`spawn`](./fn.spawn.html), except `U` must also implement the
/// [`Aggregator<I>`](./trait.Aggregator.html) trait in addition to [`CongAlg<I>`](./trait.CongAlg.html).
pub fn spawn_aggregator<I, U>(backend_builder: BackendBuilder<I>, cfg: Config<I, U>) -> CCPHandle
where
    I: Ipc,
    U: CongAlg<I> + Aggregator<I>,
{
    let stop_signal = Arc::new(atomic::AtomicBool::new(true));
    CCPHandle {
        continue_listening: stop_signal.clone(),
        join_handle: thread::spawn(move || {
            run_aggregator_inner(backend_builder, &cfg, stop_signal.clone())
        })
    }
}

fn send_and_install<I>(sock_id: u32, sender: BackendSender<I>, bin: Bin, sc: Scope) -> Result<()>
where
    I: Ipc
{
    let msg = serialize::install::Msg {
        sid: sock_id,
        program_uid: sc.program_uid,
        num_events: bin.events.len() as u32,
        num_instrs: bin.instrs.len() as u32,
        instrs: bin,
    };
    let buf = serialize::serialize(&msg)?;
    sender.send_msg(&buf[..])?;
    Ok(())
}

fn install_programs<I, U>(backend: BackendSender<I>, scope_map: &mut HashMap<String, Scope>) -> Result<()>
where
    I: Ipc,
    U: CongAlg<I>,
{
    let programs = U::init_programs();
    for (program_name, program) in programs.iter() {

        match lang::compile(program.as_bytes(), &[]) {
            Ok((bin, sc)) => {
                match send_and_install(0, backend.clone(), bin, sc.clone()) {
                    Ok(_) => {},
                    Err(e) => {
                        return Err(Error(format!("Failed to install datapath program \"{}\": {:?}", program_name, e)));
                    },
                }
                scope_map.insert(program_name.to_string(), sc.clone());
            }
            Err(e) => {
                return Err(Error(format!("Datapath program \"{}\" failed to compile: {:?}", program_name, e)));
            }
        }
    }
    Ok(())
}

// Main execution inner loop of ccp.
// Blocks "forever", or until the iterator stops iterating.
//
// `run_inner()`:
// 1. listens for messages from the datapath
// 2. call the appropriate message in `U: impl CongAlg`
// The function can return for two reasons: an error, or the iterator returned None.
// The latter should only happen for spawn(), and not for run().
// It returns any error, either from:
// 1. the IPC channel failing
// 2. Receiving an install control message (only the datapath should receive these).
fn run_inner<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>, continue_listening: Arc<atomic::AtomicBool>)  -> Result<()>
where
    I: Ipc,
    U: CongAlg<I>,
{
    let mut receive_buf = [0u8; 1024];
    let mut  b = backend_builder.build(continue_listening.clone(), &mut receive_buf[..]);
    let mut flows = HashMap::<u32, U>::new();
    let backend = b.sender();

    cfg.logger.as_ref().map(|log| {
        info!(log, "starting CCP";
            "algorithm" => U::name(),
            "ipc"       => I::name(),
        );
    });

    let mut scope_map = Rc::new(HashMap::<String, Scope>::new());

    match install_programs::<I, U>(backend.clone(), Rc::get_mut(&mut scope_map).unwrap()) {
        Ok(_) => {}
        Err(msg) => { return Err(msg) }
    }

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
                        sender: backend.clone(),
                        programs: scope_map.clone(),
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
                        alg.on_report(m.sid, Report {
                            program_uid: m.program_uid,
                            fields: m.fields 
                        })
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

use std::net::{SocketAddr, UdpSocket};
use serde::{Serialize, Deserialize};
use rmps::{Serializer, Deserializer};
use std::io::ErrorKind;
// Aggregate congestion control version of main inner execution loop of CCP
// Blocks "forever", or until the iterator stops iterating. 
// 
// Maps flow ids to keys (as defined by user-provided Aggregator). Only a single instance of `U` is created for each aggregate.
// When a new flow has a key that we have not yet seen before, we create an instance of `U` (this only uses the `CongAlg` trait)
// When a new flow has a key that has already been mapped, we retrieve the instance of `U` and call `U::new_flow`
// See [`run_inner`](./fn.run_inner.html) for more details about return codes. 
fn run_aggregator_inner<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>, continue_listening: Arc<atomic::AtomicBool>) -> Result<()>
where
    I: Ipc,
    U: CongAlg<I> + Aggregator<I>,
{
    let mut receive_buf = [0u8; 1024];
    let mut  b = backend_builder.build(continue_listening.clone(), &mut receive_buf[..]);
    let mut flows = HashMap::<u32, U::Key>::new();
    let mut aggregates = HashMap::<U::Key, U>::new();
    let mut num_flows_per_agg = HashMap::<U::Key, u32>::new();
    let backend = b.sender();

    cfg.logger.as_ref().map(|log| {
        info!(log, "starting CCP";
            "algorithm" => U::name(),
            "ipc"       => I::name(),
        );
    });

    let mut scope_map = Rc::new(HashMap::<String, Scope>::new());

    match install_programs::<I, U>(backend.clone(), Rc::get_mut(&mut scope_map).unwrap()) {
        Ok(_) => {}
        Err(msg) => { return Err(msg) }
    }

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

                let info = DatapathInfo {
                        sock_id: c.sid,
                        init_cwnd: c.init_cwnd,
                        mss: c.mss,
                        src_ip: c.src_ip,
                        src_port: c.src_port,
                        dst_ip: c.dst_ip,
                        dst_port: c.dst_port,
                };

                let k = U::Key::from(info);

                aggregates.get_mut(&k).and_then(|agg| {
                    agg.new_flow(Datapath {
                        sock_id: c.sid,
                        sender: backend.clone(),
                        programs: scope_map.clone(),
                    }, info);
                    Some(())
                }).or_else(|| {
                    let agg = U::create(
                        Datapath{
                            sock_id: c.sid,
                            sender: backend.clone(),
                            programs: scope_map.clone(),
                        },
                        cfg.clone(),
                        info
                    );
                    aggregates.insert(k, agg);
                    Some(())
                });

                flows.insert(c.sid, k);
                *num_flows_per_agg.entry(k).or_insert(0) += 1;

            }
            Msg::Ms(m) => {
                if flows.contains_key(&m.sid) {
                    if m.num_fields == 0 {
                        let mut key = flows.remove(&m.sid).unwrap();
                        if aggregates.contains_key(&key) {
                            let num_flows = num_flows_per_agg.get_mut(&key).unwrap();
                            *num_flows -= 1;
                            if *num_flows == 0 {
                                aggregates.remove(&key).unwrap();
                            }
                            aggregates.get_mut(&key).and_then(|agg| {
                                agg.close_one(&key);
                                if *num_flows == 0 {
                                    agg.close();
                                }
                                Some(())
                            });
                        } else {
                            eprintln!("error: unknown aggregate key {:?}!", key);
                        }
                    } else {
                        let mut key = flows.get_mut(&m.sid).unwrap();
                        aggregates.get_mut(&key).and_then(move |agg| {
                            agg.on_report(m.sid, Report {
                                program_uid: m.program_uid,
                                fields: m.fields
                            });
                            Some(())
                        }).or_else(|| {
                            eprintln!("error: unknown aggregate key {:?}!", key);
                            Some(())
                        });
                    }
                } else {
                    cfg.logger.as_ref().map(|log| {
                        debug!(log, "measurement for unknown flow"; "sid" => m.sid);
                    });
                }
            }
            Msg::Ins(_) => {
                unimplemented!()
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


// Cluster congestion control version of main inner execution loop of CCP
// Blocks "forever", or until the iterator stop iterating.
//
// Backend *must* use non-blocking sockets for this to work, because it polls 
// a (non-blocking) UDP socket to the controller and a timer all in the same thread.
fn run_aggregator_with_remote_inner<I, U>(backend_builder: BackendBuilder<I>, cfg: &Config<I, U>, continue_listening: Arc<atomic::AtomicBool>, local_addr : String, controller_addr : String) -> Result<()>
where
    I: Ipc,
    U: CongAlg<I> + Aggregator<I> + Slave,
{
    let mut receive_buf = [0u8; 1024];
    let mut allocation_buf = [0u8;1024];
    let mut summary_buf = Vec::new();
    let mut  b = backend_builder.build(continue_listening.clone(), &mut receive_buf[..]);
    let backend = b.sender();
    let mut host_aggregator : Option<U> = None;
    let mut flows = HashSet::<u32>::new();

    cfg.logger.as_ref().map(|log| {
        info!(log, "starting CCP";
            "algorithm" => U::name(),
            "ipc"       => I::name(),
            "remote"    => &controller_addr,
        );
    });

    let mut scope_map = Rc::new(HashMap::<String, Scope>::new());

    match install_programs::<I, U>(backend.clone(), Rc::get_mut(&mut scope_map).unwrap()) {
        Ok(_) => {}
        Err(msg) => { return Err(msg) }
    }

    let local_addr_s : SocketAddr = local_addr.parse().expect("failed to parse local address");   
    let controller_addr_s : SocketAddr = controller_addr.parse().expect("faile to parse remote address");
    let controller = UdpSocket::bind(local_addr_s).expect("failed to bind to udp socket");
    controller.set_nonblocking(true).expect("failed to set non-blocking");

    let mut next_summary_time = 0;
    loop {
        if let Some(msg) = b.next_nonblocking() {
            match msg {
                Msg::Cr(c) => {
                    if flows.contains(&c.sid) {
                        cfg.logger.as_ref().map(|log| {
                            warn!(log, "re-creating already created flow"; "sid" => c.sid);
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

                    let info = DatapathInfo {
                            sock_id: c.sid,
                            init_cwnd: c.init_cwnd,
                            mss: c.mss,
                            src_ip: c.src_ip,
                            src_port: c.src_port,
                            dst_ip: c.dst_ip,
                            dst_port: c.dst_port,
                    };

                    if let Some(ref mut agg) = host_aggregator {
                        agg.new_flow(Datapath {
                            sock_id: c.sid,
                            sender: backend.clone(),
                            programs: scope_map.clone(),
                        }, info);
                    } else {
                        host_aggregator = Some(
                            U::create(
                                Datapath{
                                    sock_id: c.sid,
                                    sender: backend.clone(),
                                    programs: scope_map.clone(),
                                },
                                cfg.clone(),
                                info
                            )
                        )
                    }
                }
                Msg::Ms(r) => {
                    if let Some(ref mut agg) = host_aggregator {
                        if r.num_fields == 0 {
                            flows.remove(&r.sid);
                            agg.close_one(r.sid);
                            if flows.len() <= 0 {
                                agg.close();
                            }
                        } else {
                            let send_sum_now = Slave::on_report(agg, r.sid, Report {
                                program_uid: r.program_uid,
                                fields: r.fields,
                            });
                            if send_sum_now {
                                let sum = agg.create_summary();
                                sum.serialize(&mut Serializer::new(&mut summary_buf)).expect("failed to serialize summary msg");
                                controller.send_to(&summary_buf[..], controller_addr_s).expect("failed to send immediate summary to controller");
                            }
                        }
                    } else {
                        cfg.logger.as_ref().map(|log| {
                            warn!(log, "received report but aggregate hasn't been created yet!"; "sid" => r.sid);
                        });
                    }
                }
                Msg::Ins(_) => {
                    unimplemented!()
                }
                _ => continue,
            }
        };
        match controller.recv(&mut allocation_buf) {
            Ok(amt) => {
                if amt > 0 {
                    let mut de = Deserializer::new(&allocation_buf[..amt]);
                    let alloc : Allocation = Deserialize::deserialize(&mut de).expect("failed to deserialize allocation msg");
                    if let Some(ref mut agg) = host_aggregator {
                        agg.on_allocation(alloc);
                    } else {
                        cfg.logger.as_ref().map(|log| {
                            warn!(log, "received allocation but aggregate hasn't been created yet!"; "sid" => alloc.id);
                        });
                    }
                 }
            }
            Err(ref err) if err.kind() != ErrorKind::WouldBlock => { panic!("UDP socket error: {}", err) }
            Err(_) => { panic!("UDP socket error: unknown"); }
        }
        if next_summary_time > time::precise_time_ns() {
            // send summary
            if let Some(ref mut agg) = host_aggregator {
                let sum = agg.create_summary();
                sum.serialize(&mut Serializer::new(&mut summary_buf)).expect("failed to serialize summary msg");
                controller.send_to(&summary_buf[..], controller_addr_s).expect("failed to send summary to controller");
                next_summary_time = time::precise_time_ns() + (agg.next_summary_time() as u64 * 1000);
            } else {
                cfg.logger.as_ref().map(|log| {
                    warn!(log, "need to send summary but aggregate hasn't been created yet!"; "next_summary_time" => next_summary_time);
                });
            }
        }
        // if the thread has been killed, return that as error
        if !continue_listening.load(atomic::Ordering::SeqCst) {
            return Ok(())
        }
    }
}



#[cfg(test)]
mod test;
