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
//! use std::collections::HashMap;
//! use portus::{CongAlg, Flow, Datapath, DatapathInfo, DatapathTrait, Report};
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

use std::collections::HashMap;
use std::rc::Rc;

pub mod ipc;
pub mod lang;
pub mod serialize;
pub mod test_helper;
#[macro_use]
pub mod algs;
mod errors;
pub use crate::errors::*;
pub use portus_export::register_ccp_alg;

use crate::ipc::BackendSender;
use crate::ipc::Ipc;
use crate::lang::{Reg, Scope};

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
                                Reg::Control(idx, ref t, v) => {
                                    Ok((Reg::Control(idx, t.clone(), v), u64::from(new_value)))
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
                        Reg::Control(idx, ref t, v) => {
                            Ok((Reg::Control(idx, t.clone(), v), u64::from(new_value)))
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
    pub from: String,
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

/// Tell `portus` how to construct instances of your `impl` [`portus::CongAlg`].
///
/// You should also annotate your struct with [`portus_export::register_ccp_alg`]()).
pub trait CongAlgBuilder<'a> {
    /// This function should return a new
    /// [`clap::App`](https://docs.rs/clap/2.32.0/clap/struct.App.html) that describes the
    /// arguments this algorithm needs to create an instance of itself.
    fn args() -> clap::App<'a>;

    /// This function takes as input the set of parsed arguments and uses them to parameterize a
    /// new instance of this congestion control algorithm. The matches will be derived from
    /// running `Clap::App::get_matches_from` on the `clap::App` returned by the `register` function.
    /// It also takes an instsance of a logger so that the calling program can define the logging
    /// behavior (eg. format and redirection).
    fn with_arg_matches(args: &clap::ArgMatches) -> Result<Self>
    where
        Self: Sized;
}

mod run;
pub use run::*;

#[cfg(test)]
mod test;
