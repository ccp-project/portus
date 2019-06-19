//! Welcome to CCP.
//!
//! This crate, portus, implements a CCP. This includes:
//! 1. An interface definition for external types wishing to implement congestion control
//!    algorithms (`CongAlg`).
//! 2. A [compiler](lang/index.html) for datapath programs.
//! 3. An IPC and serialization [layer](ipc/index.html) for communicating with libccp-compliant datapaths.
//!
//! The entry points into portus are [`run`](./fn.run.html) and [`spawn`](./fn.spawn.html), which start
//! the CCP algorithm runtime. There is also the convenience macro [`start`](./macro.start.html).
//!
//! The runtime listens for datapath messages and dispatches calls to
//! the appropriate congestion control methods.
//!
//! Example
//! =======
//!
//! The following congestion control algorithm sets the congestion window to `42`, and prints the
//! minimum RTT observed over 42 millisecond intervals.
//!
//! ```
//! extern crate fnv;
//! extern crate portus;
//! use std::collections::HashMap;
//! use portus::{CongAlg, Flow, Config, Datapath, DatapathInfo, DatapathTrait, Report};
//! use portus::ipc::Ipc;
//! use portus::lang::Scope;
//! use portus::lang::Bin;
//!
//! #[derive(Clone, Default)]
//! struct MyCongestionControlAlgorithm(Scope);
//!
//! impl<I: Ipc> CongAlg<I> for MyCongestionControlAlgorithm {
//!     type Flow = Self;
//!
//!     fn name() -> &'static str {
//!         "My congestion control algorithm"
//!     }
//!     fn datapath_programs(&self) -> HashMap<&'static str, String> {
//!         let mut h = HashMap::default();
//!         h.insert(
//!             "MyProgram", "
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
//!             ".to_owned(),
//!         );
//!         h
//!     }
//!     fn new_flow(&self, mut control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
//!         let sc = control.set_program("MyProgram", None).unwrap();
//!         MyCongestionControlAlgorithm(sc)
//!     }
//! }
//! impl Flow for MyCongestionControlAlgorithm {
//!     fn on_report(&mut self, sock_id: u32, m: Report) {
//!         println!("minrtt: {:?}", m.get_field("Report.minrtt", &self.0).unwrap());
//!     }
//! }
//! ```

#![feature(box_patterns)]
#![feature(integer_atomics)]
#![feature(never_type)]
#![feature(stmt_expr_attributes)]
#![feature(test)]

extern crate bytes;
extern crate clap;
extern crate crossbeam;
extern crate fnv;
extern crate libc;
extern crate nix;
#[macro_use]
extern crate nom;
extern crate time;

#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{atomic, Arc};
use std::thread;

pub mod ipc;
pub mod lang;
pub mod serialize;
pub mod test_helper;
#[macro_use]
pub mod algs;
mod errors;
pub use crate::errors::*;

use crate::ipc::Ipc;
use crate::ipc::{BackendBuilder, BackendSender};
use crate::lang::{Bin, Reg, Scope};
use crate::serialize::Msg;

/// CCP custom `Result` type, using `Error` as the `Err` type.
pub type Result<T> = std::result::Result<T, Error>;

/// A collection of methods to interact with the datapath.
pub trait DatapathTrait {
    fn get_sock_id(&self) -> u32;
    /// Tell datapath to use a preinstalled program.
    fn set_program(
        &mut self,
        program_name: &'static str,
        fields: Option<&[(&str, u32)]>,
    ) -> Result<Scope>;
    /// Update the value of a register in an already-installed fold function.
    fn update_field(&self, sc: &Scope, update: &[(&str, u32)]) -> Result<()>;
}

/// A collection of methods to interact with the datapath.
#[derive(Clone)]
pub struct Datapath<T: Ipc> {
    sock_id: u32,
    sender: BackendSender<T>,
    programs: Rc<HashMap<String, Scope>>,
}

impl<T: Ipc> DatapathTrait for Datapath<T> {
    fn get_sock_id(&self) -> u32 {
        self.sock_id
    }

    fn set_program(
        &mut self,
        program_name: &'static str,
        fields: Option<&[(&str, u32)]>,
    ) -> Result<Scope> {
        // if the program with this key exists, return it; otherwise return nothing
        match self.programs.get(program_name) {
            Some(sc) => {
                // apply optional updates to values of registers in this scope
                let fields: Vec<(Reg, u64)> = fields
                    .unwrap_or_else(|| &[])
                    .iter()
                    .map(|&(reg_name, new_value)| {
                        if reg_name.starts_with("__") {
                            return Err(Error(format!(
                                "Cannot update reserved field: {:?}",
                                reg_name
                            )));
                        }

                        sc.get(reg_name)
                            .ok_or_else(|| Error(format!("Unknown field: {:?}", reg_name)))
                            .and_then(|reg| match *reg {
                                Reg::Control(idx, ref t) => {
                                    Ok((Reg::Control(idx, t.clone()), u64::from(new_value)))
                                }
                                Reg::Implicit(idx, ref t) if idx == 4 || idx == 5 => {
                                    Ok((Reg::Implicit(idx, t.clone()), u64::from(new_value)))
                                }
                                _ => Err(Error(format!("Cannot update field: {:?}", reg_name))),
                            })
                    })
                    .collect::<Result<_>>()?;
                let msg = serialize::changeprog::Msg {
                    sid: self.sock_id,
                    program_uid: sc.program_uid,
                    num_fields: fields.len() as u32,
                    fields,
                };
                let buf = serialize::serialize(&msg)?;
                self.sender.send_msg(&buf[..])?;
                Ok(sc.clone())
            }
            _ => Err(Error(format!(
                "Map does not contain datapath program with key: {:?}",
                program_name
            ))),
        }
    }

    fn update_field(&self, sc: &Scope, update: &[(&str, u32)]) -> Result<()> {
        let fields: Vec<(Reg, u64)> = update
            .iter()
            .map(|&(reg_name, new_value)| {
                if reg_name.starts_with("__") {
                    return Err(Error(format!(
                        "Cannot update reserved field: {:?}",
                        reg_name
                    )));
                }

                sc.get(reg_name)
                    .ok_or_else(|| Error(format!("Unknown field: {:?}", reg_name)))
                    .and_then(|reg| match *reg {
                        Reg::Control(idx, ref t) => {
                            Ok((Reg::Control(idx, t.clone()), u64::from(new_value)))
                        }
                        Reg::Implicit(idx, ref t) if idx == 4 || idx == 5 => {
                            Ok((Reg::Implicit(idx, t.clone()), u64::from(new_value)))
                        }
                        _ => Err(Error(format!("Cannot update field: {:?}", reg_name))),
                    })
            })
            .collect::<Result<_>>()?;

        let msg = serialize::update_field::Msg {
            sid: self.sock_id,
            num_fields: fields.len() as u8,
            fields,
        };

        let buf = serialize::serialize(&msg)?;
        self.sender.send_msg(&buf[..])?;
        Ok(())
    }
}

fn send_and_install<I>(sock_id: u32, sender: &BackendSender<I>, bin: Bin, sc: &Scope) -> Result<()>
where
    I: Ipc,
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

/// Configuration parameters for the portus runtime.
/// Defines a `slog::Logger` to use for (optional) logging
#[derive(Clone, Default)]
pub struct Config {
    pub logger: Option<slog::Logger>,
}

/// The set of information passed by the datapath to CCP
/// when a connection starts. It includes a unique 5-tuple (CCP socket id + source and destination
/// IP and port), the initial congestion window (`init_cwnd`), and flow MSS.
#[derive(Debug, Clone)]
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
        if sc.program_uid != self.program_uid {
            return Err(Error::from(StaleProgramError));
        }

        match sc.get(field) {
            Some(r) => match *r {
                Reg::Report(idx, _, _) => {
                    if idx as usize >= self.fields.len() {
                        Err(Error::from(InvalidReportError))
                    } else {
                        Ok(self.fields[idx as usize])
                    }
                }
                _ => Err(Error::from(InvalidRegTypeError)),
            },
            None => Err(Error::from(FieldNotFoundError)),
        }
    }
}

/// Implement this trait, [`portus::CongAlg`](./trait.CongAlg.html), and
///[`portus::CongAlgBuilder`](./trait.CongAlgBuilder.html) to define a CCP congestion control
/// algorithm.
///
/// * `CongAlg` implements functionality which applies to a given algorithm as a whole
/// * `Flow` implements functionality specific to an individual flow
/// * `CongAlgBuilder` specifies how the trait that implements `CongAlg` should be built
/// from given command-line arguments.
pub trait Flow {
    /// This callback specifies the algorithm's behavior when it receives a report
    /// of measurements from the datapath.
    fn on_report(&mut self, sock_id: u32, m: Report);

    /// Optionally specify what the algorithm should do when the flow ends,
    /// e.g., clean up any external resources.
    /// The default implementation does nothing.
    fn close(&mut self) {}
}

impl<T> Flow for Box<T>
where
    T: Flow + ?Sized,
{
    fn on_report(&mut self, sock_id: u32, m: Report) {
        T::on_report(self, sock_id, m)
    }

    fn close(&mut self) {
        T::close(self)
    }
}

/// implement this trait, [`portus::CongAlgBuilder`](./trait.CongAlgBuilder.html) and
/// [`portus::Flow`](./trait.Flow.html) to define a ccp congestion control algorithm.
///
/// * `CongAlg` implements functionality which applies to a given algorithm as a whole
/// * `Flow` implements functionality specific to an individual flow
/// * `CongAlgBuilder` specifies how the trait that implements `CongAlg` should be built
/// from given command-line arguments.
pub trait CongAlg<I: Ipc> {
    /// A type which implements the [`portus::Flow`](./trait.Flow.html) trait, to manage
    /// an individual connection.
    type Flow: Flow;

    /// A unique name for the algorithm.
    fn name() -> &'static str;

    /// `datapath_programs` returns all datapath programs the congestion control algorithm
    /// will to use during its execution. It is called once, when Portus initializes
    /// ([`portus::run`](./fn.run.html) or [`portus::spawn`](./fn.spawn.html)).
    ///
    /// It should return a vector of string tuples, where the first string in each tuple is a unique name
    /// identifying the program, and the second string is the code for the program itself.
    ///
    /// The Portus runtime will panic if any of the datapath programs do not compile.
    ///
    /// For example,
    /// ```
    /// extern crate fnv;
    /// use std::collections::HashMap;
    /// let mut h = HashMap::new();
    /// h.insert("prog1", "...(program)...".to_string());
    /// h.insert("prog2", "...(program)...".to_string());
    /// ```
    fn datapath_programs(&self) -> HashMap<&'static str, String>;

    /// Create a new instance of the CongAlg to manage a new flow.
    /// Optionally copy any configuration parameters from `&self`.
    fn new_flow(&self, control: Datapath<I>, info: DatapathInfo) -> Self::Flow;
}

/// Structs implementing [`portus::CongAlg`](./trait.CongAlg.html) must also implement this trait
/// (and must be annotated with [`portus_export::register_ccp_alg`]())
///
/// The expected use of this trait in a calling program is as follows:
/// ```no-run
/// let args = CongAlgBuilder::args();
/// let matches = app.get_matches_from(std::env::args_os());
/// let alg = CongAlgBuilder::with_arg_matches(matches);
/// ```
pub trait CongAlgBuilder<'a, 'b> {
    /// This function should return a new
    /// [`clap::App`](https://docs.rs/clap/2.32.0/clap/struct.App.html) that describes the
    /// arguments this algorithm needs to create an instance of itself.
    fn args() -> clap::App<'a, 'b>;

    /// This function takes as input the set of parsed arguments and uses them to parameterize a
    /// new instance of this congestion control algorithm. The matches will be derived from
    /// running `Clap::App::get_matches_from` on the `clap::App` returned by the `register` function.
    /// It also takes an instsance of a logger so that the calling program can define the logging
    /// behavior (eg. format and redirection).
    fn with_arg_matches(args: &clap::ArgMatches, logger: Option<slog::Logger>) -> Result<Self>
    where
        Self: Sized;
}

/// A handle to manage running instances of the CCP execution loop.
#[derive(Debug)]
pub struct CCPHandle {
    pub continue_listening: Arc<atomic::AtomicBool>,
    pub join_handle: thread::JoinHandle<Result<()>>,
}

impl CCPHandle {
    /// Instruct the execution loop to exit.
    pub fn kill(&self) {
        self.continue_listening
            .store(false, atomic::Ordering::SeqCst);
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
pub fn run<I, U>(backend_builder: BackendBuilder<I>, cfg: Config, alg: U) -> Result<!>
where
    I: Ipc,
    U: CongAlg<I>,
{
    // call run_inner
    match run_inner(
        Arc::new(atomic::AtomicBool::new(true)),
        backend_builder,
        cfg,
        alg,
    ) {
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
pub fn spawn<I, U>(backend_builder: BackendBuilder<I>, cfg: Config, alg: U) -> CCPHandle
where
    I: Ipc,
    U: CongAlg<I> + 'static + Send,
{
    let stop_signal = Arc::new(atomic::AtomicBool::new(true));
    CCPHandle {
        continue_listening: stop_signal.clone(),
        join_handle: thread::spawn(move || run_inner(stop_signal, backend_builder, cfg, alg)),
    }
}

use crate::ipc::Backend;
use crate::ipc::SingleBackend;
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
fn run_inner<I, U>(
    continue_listening: Arc<atomic::AtomicBool>,
    backend_builder: BackendBuilder<I>,
    cfg: Config,
    alg: U,
) -> Result<()>
where
    I: Ipc,
    U: CongAlg<I>,
{
    let mut b = backend_builder.build::<SingleBackend<I>>(continue_listening.clone());
    let mut flows = HashMap::<u32, U::Flow>::default();
    let backend = b.sender();

    if let Some(log) = cfg.logger.as_ref() {
        info!(log, "starting CCP";
            "algorithm" => U::name(),
            "ipc"       => I::name(),
        );
    }

    let mut scope_map = Rc::new(HashMap::<String, Scope>::default());

    let programs = alg.datapath_programs();
    for (program_name, program) in programs.iter() {
        match lang::compile(program.as_bytes(), &[]) {
            Ok((bin, sc)) => {
                match send_and_install(0, &backend, bin, &sc) {
                    Ok(_) => {}
                    Err(e) => {
                        return Err(Error(format!(
                            "Failed to install datapath program \"{}\": {:?}",
                            program_name, e
                        )));
                    }
                }
                Rc::get_mut(&mut scope_map)
                    .unwrap()
                    .insert(program_name.to_string(), sc.clone());
            }
            Err(e) => {
                return Err(Error(format!(
                    "Datapath program \"{}\" failed to compile: {:?}",
                    program_name, e
                )));
            }
        }
    }

    while let Some(msg) = b.next() {
        match msg {
            Msg::Cr(c) => {
                if flows.remove(&c.sid).is_some() {
                    if let Some(log) = cfg.logger.as_ref() {
                        debug!(log, "re-creating already created flow"; "sid" => c.sid);
                    }
                }

                if let Some(log) = cfg.logger.as_ref() {
                    debug!(log, "creating new flow";
                           "sid" => c.sid,
                           "init_cwnd" => c.init_cwnd,
                           "mss"  =>  c.mss,
                           "src_ip"  =>  c.src_ip,
                           "src_port"  =>  c.src_port,
                           "dst_ip"  =>  c.dst_ip,
                           "dst_port"  =>  c.dst_port,
                    );
                }

                let f = alg.new_flow(
                    Datapath {
                        sock_id: c.sid,
                        sender: backend.clone(),
                        programs: scope_map.clone(),
                    },
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
                flows.insert(c.sid, f);
            }
            Msg::Ms(m) => {
                if flows.contains_key(&m.sid) {
                    if m.num_fields == 0 {
                        let mut alg = flows.remove(&m.sid).unwrap();
                        alg.close();
                    } else {
                        let alg = flows.get_mut(&m.sid).unwrap();
                        alg.on_report(
                            m.sid,
                            Report {
                                program_uid: m.program_uid,
                                fields: m.fields,
                            },
                        )
                    }
                } else if let Some(log) = cfg.logger.as_ref() {
                    debug!(log, "measurement for unknown flow"; "sid" => m.sid);
                }
            }
            Msg::Ins(_) => {
                unreachable!()
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
