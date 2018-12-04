use fnv::FnvHashMap as HashMap;

use portus;
use pyo3::prelude::*;
use slog;
use std::rc::Rc;

use portus::{CongAlg, Datapath, Report, Flow, DatapathTrait};
use portus::ipc::Ipc;
// use super::{py_create_flow, py_setattr};
use super::{PyDatapath, PyReport, DatapathInfo};

pub struct PyFlow {
    py: Python<'static>,
    logger: slog::Logger,
    debug: bool,
    flow_obj: PyObject,
    datapath: Py<PyDatapath>,
}

pub struct PyCongAlg {
    pub py: Python<'static>,
    pub logger: slog::Logger,
    pub debug: bool,
    pub alg_obj : PyObject,
}


impl Flow for PyFlow {
    fn on_report(&mut self, sock_id: u32, m: Report) {
        let py = self.py;

        if self.debug {
            debug!(self.logger, "Got report";
                "sid" => sock_id,
            );
        }

        let datapath: &PyDatapath = self.datapath.as_ref(py);
        let report = match datapath.sc {
            Some(ref s) => {
                if m.program_uid != s.program_uid {
                    if self.debug {
                        debug!(self.logger, "Report is stale, ignoring...";
                           "sid"        => sock_id,
                           "report_uid" => m.program_uid,
                           "scope_uid"  => s.program_uid,
                       )
                    }
                    return;
                }
                let rep = py
                    .init(|_t| PyReport {
                        report: m,
                        sc: Rc::downgrade(s),
                    })
                    .unwrap_or_else(|e| {
                        e.print(py);
                        panic!("Failed to create PyReport")
                    });
                rep
            }
            None => {
                error!(self.logger, "Failed to get report: can't find scope (no datapath program installed yet)";
                   "sid" => sock_id,
                );
                return;
            }
        };
        let args = PyTuple::new(py, &[report]);
        match self.flow_obj.call_method1(py, "on_report", args) {
            Ok(_ret) => {}
            Err(e) => {
                e.print(py);

                error!(self.logger, "on_report() failed to complete";
                   "sid" => sock_id,
                );
            }
        };
    }
}

fn string_to_static_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl<T: Ipc> CongAlg<T> for PyCongAlg {
    type Flow = PyFlow;

    fn name() -> &'static str {
        "python"
    }

    fn datapath_programs(&self) -> HashMap<&'static str, String> {
        let py = self.py;

        match self.alg_obj.call_method0(py, "datapath_programs") {
            Ok(ret) => {
                let dict: &PyDict = match ret.extract(py) {
                    Ok(l) => l,
                    Err(e) => {
                        e.print(py);
                        panic!("datapath_programs() must return a *list* of tuples of (2) strings.\nreturn value was not a list.")
                    }
                };
                dict.iter().map(|(key, value)| {
                    let program_name = match PyString::try_from(key) {
                        Ok(pn) => pn.to_string_lossy().into_owned(),
                        Err(_) => {
                            panic!("datapath_programs() must return a list of tuples of (2) *strings*.\ngot a list of tuples, but the first element was not a string.")
                        }
                    };
                    let program_string = match PyString::try_from(value) {
                        Ok(ps) => ps.to_string_lossy().into_owned(),
                        Err(_) => {
                            panic!("datapath_programs() must return a list of tuples of (2) *strings*.\ngot a list of tuples, but the second element was not a string.")
                        }
                    };
                    
                    (string_to_static_str(program_name), program_string)
                }).collect()
            }
            Err(e) => {
                e.print(py);
                panic!("error calling datapath_programs()");
            }
        }
    }

    fn new_flow(&self, control: Datapath<T>, info: portus::DatapathInfo) -> Self::Flow {
        let py = self.py;

        if self.debug {
            debug!(self.logger, "New flow"; "sid" => control.get_sock_id());
        }

        let py_datapath = py
            .init(|_| PyDatapath {
                sock_id: control.get_sock_id(),
                backend: Box::new(control.clone()),
                logger: self.logger.clone(),
                sc: Default::default(),
                debug: self.debug.clone(),
            })
            .unwrap_or_else(|e| {
                e.print(py);
                panic!("Failed to create PyDatapath");
            });

        let py_info = py
            .init(|_| DatapathInfo {
                sock_id: info.sock_id,
                init_cwnd: info.init_cwnd,
                mss: info.mss,
                src_ip: info.src_ip,
                src_port: info.src_port,
                dst_ip: info.dst_ip,
                dst_port: info.dst_port,
            })
            .unwrap_or_else(|e| {
                e.print(py);
                panic!("Failed to create DatapathInfo")
            });

        let kwargs = PyDict::new(py);
        let _ = kwargs.set_item("datapath", &py_datapath).unwrap_or_else(|e| {
            e.print(py);
            panic!("error setting kwargs.datapath");
        });
        let _ = kwargs.set_item("datapath_info", py_info).unwrap_or_else(|e| {
            e.print(py);
            panic!("error setting kwargs.datapath_info");
        });
        let flow_obj = self.alg_obj.call_method(py, "new_flow", (), kwargs).unwrap_or_else(|e| {
            e.print(py);
            panic!("error calling new_flow()");
        });

        PyFlow {
            py : self.py,
            debug : self.debug,
            logger: self.logger.clone(),
            flow_obj,
            datapath: py_datapath,
        }
    }

    // TODO implement close, deallocate memory from class
}
