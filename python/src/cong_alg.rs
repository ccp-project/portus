use super::{DatapathInfo, PyDatapath, PyReport};
use portus::ipc::Ipc;
use portus::{CongAlg, Datapath, DatapathTrait, Flow, Report};
use pyo3::prelude::*;
use pyo3::types::*;
use std::collections::HashMap;
use std::rc::Rc;

pub struct PyFlow<'py> {
    py: Python<'py>,
    flow_obj: PyObject,
    datapath: Py<PyDatapath>,
}

impl<'py> Flow for PyFlow<'py> {
    fn on_report(&mut self, sock_id: u32, m: Report) {
        let py = self.py;

        tracing::debug!(?sock_id, "Got report");

        let datapath: &PyDatapath = &self.datapath.borrow(py);
        let report = match datapath.sc {
            Some(ref s) => {
                if m.program_uid != s.program_uid {
                    tracing::debug!(?sock_id, ?m.program_uid, ?s.program_uid, "Report is stale, ignoring");
                    return;
                }

                let rep = Py::new(
                    py,
                    PyReport {
                        report: m,
                        sc: Rc::downgrade(s),
                    },
                )
                .unwrap_or_else(|e| {
                    e.print(py);
                    panic!("Failed to create PyReport")
                });
                rep
            }
            None => {
                tracing::error!(
                    ?sock_id,
                    "Failed to get report: can't find scope (no datapath program installed yet)"
                );
                return;
            }
        };

        let args = PyTuple::new(py, &[report]);
        match self.flow_obj.call_method1(py, "on_report", args) {
            Ok(_ret) => {}
            Err(e) => {
                e.print(py);
                tracing::error!(sock_id, "on_report() failed to complete");
            }
        };
    }
}

pub struct PyCongAlg<'py> {
    pub py: Python<'py>,
    pub alg_obj: PyObject,
}

impl<'py, T: Ipc> CongAlg<T> for PyCongAlg<'py> {
    type Flow = PyFlow<'py>;

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
                    let program_name = match <PyString as PyTryFrom<'_>>::try_from(key) {
                        Ok(pn) => pn.to_string_lossy().into_owned(),
                        Err(_) => {
                            panic!("datapath_programs() must return a list of tuples of (2) *strings*.\ngot a list of tuples, but the first element was not a string.")
                        }
                    };
                    let program_string = match <PyString as PyTryFrom<'_>>::try_from(value) {
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
        tracing::debug!(sock_id = ?control.get_sock_id(), "New flow");
        let py_datapath = Py::new(
            py,
            PyDatapath {
                sock_id: control.get_sock_id(),
                backend: Box::new(control),
                sc: Default::default(),
            },
        )
        .unwrap_or_else(|e| {
            e.print(py);
            panic!("Failed to create PyDatapath");
        });

        let py_info = Py::new(
            py,
            DatapathInfo {
                sock_id: info.sock_id,
                init_cwnd: info.init_cwnd,
                mss: info.mss,
                src_ip: info.src_ip,
                src_port: info.src_port,
                dst_ip: info.dst_ip,
                dst_port: info.dst_port,
            },
        )
        .unwrap_or_else(|e| {
            e.print(py);
            panic!("Failed to create DatapathInfo")
        });

        let kwargs = PyDict::new(py);
        let _ = kwargs
            .set_item("datapath", &py_datapath)
            .unwrap_or_else(|e| {
                e.print(py);
                panic!("error setting kwargs.datapath");
            });
        let _ = kwargs
            .set_item("datapath_info", py_info)
            .unwrap_or_else(|e| {
                e.print(py);
                panic!("error setting kwargs.datapath_info");
            });
        let flow_obj = self
            .alg_obj
            .call_method(py, "new_flow", (), Some(kwargs))
            .unwrap_or_else(|e| {
                e.print(py);
                panic!("error calling new_flow()");
            });

        PyFlow {
            py: self.py,
            flow_obj,
            datapath: py_datapath,
        }
    }

    // TODO implement close, deallocate memory from class
}

fn string_to_static_str(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}
