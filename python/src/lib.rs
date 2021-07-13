#![feature(box_patterns, specialization, const_fn)]
use std::rc::{Rc, Weak};

use portus::ipc;
use portus::ipc::BackendBuilder;
use portus::lang::Scope;
use portus::{DatapathTrait, Report};

extern crate simple_signal;
use simple_signal::Signal;

extern crate pyo3;
use pyo3::prelude::*;

#[macro_export]
macro_rules! raise {
    ($errtype:ident, $msg:expr) => {
        return Err(PyErr::new::<exc::$errtype, _>($msg));
    };
    ($errtype:ident, $msg:expr, false) => {
        Err(PyErr::new::<exc::$errtype, _>($msg));
    };
}

macro_rules! py_none {
    ($py:expr) => {
        PyTuple::empty($py).into_ptr()
    };
}

mod cong_alg;
use cong_alg::*;

/// Convenience wrapper around datapath struct,
/// python keeps a pointer to this for talking to the datapath
#[py::class(gc, weakref, dict)]
struct PyDatapath {
    backend: Box<DatapathTrait>,
    sc: Option<Rc<Scope>>,
    logger: slog::Logger,
    debug: bool,
    sock_id: u32,
}

// Copy of the datapath class, $[prop(get)] necessary to access the fields in python
#[py::class(gc, weakref, dict)]
pub struct DatapathInfo {
    #[prop(get)]
    pub sock_id: u32,
    #[prop(get)]
    pub init_cwnd: u32,
    #[prop(get)]
    pub mss: u32,
    #[prop(get)]
    pub src_ip: u32,
    #[prop(get)]
    pub src_port: u32,
    #[prop(get)]
    pub dst_ip: u32,
    #[prop(get)]
    pub dst_port: u32,
}

#[py::class(gc, weakref, dict)]
pub struct Measurements {
    #[prop(get)]
    pub acked: u32,
    #[prop(get)]
    pub was_timeout: bool,
    #[prop(get)]
    pub sacked: u32,
    #[prop(get)]
    pub loss: u32,
    #[prop(get)]
    pub rtt: u32,
    #[prop(get)]
    pub inflight: u32,
}

/// Convenience wrapper around the Report struct for sending to python,
/// keeps a copy of the scope so the python user doesn't need to manage it
#[py::class(gc, weakref, dict)]
struct PyReport {
    report: Report,
    sc: Weak<Scope>,
}

#[py::proto]
impl<'p> pyo3::class::PyObjectProtocol<'p> for PyReport {
    fn __getattr__(&self, name: String) -> PyResult<u64> {
        let field_name = match name.as_ref() {
            "Cwnd" | "Rate" => name.clone(),
            _ => format!("Report.{}", name),
        };
        let sc = match self.sc.upgrade() {
            Some(sc) => sc,
            None => {
                raise!(
                    Exception,
                    format!(
                        "Failed to get {}: no datapath program installed",
                        field_name.clone()
                    )
                );
            }
        };
        match self.report.get_field(field_name.as_ref(), &sc) {
            Ok(val) => Ok(val),
            Err(portus::Error(e)) => raise!(Exception, format!("Failed to get {}: {}", name, e)),
        }
    }
}

fn get_fields(list: &PyList) -> Vec<(&str, u32)> {
    list.into_iter()
        .map(|tuple_ref| {
            tuple_ref.extract()
                .expect("second argument to datapath.set_program must be a list of tuples of the form (string, int)")
        })
    .collect::<Vec<(&str, u32)>>()
}

#[py::methods]
impl PyDatapath {
    fn update_field(&self, _py: Python, reg_name: String, val: u32) -> PyResult<()> {
        if self.debug {
            debug!(self.logger, "Updating field";
                "sid" => self.sock_id,
                "field" => reg_name.clone(),
                "val" => val,
            )
        }
        let sc = match self.sc {
            Some(ref s) => s,
            None => {
                raise!(
                    ReferenceError,
                    "Cannot update field: no datapath program installed yet!"
                );
            }
        };
        match self.backend.update_field(sc, &[(reg_name.as_str(), val)]) {
            Ok(()) => Ok(()),
            Err(e) => raise!(Exception, format!("Failed to update field, err: {:?}", e)),
        }
    }

    fn update_fields(&self, py: Python, fields: &PyList) -> PyResult<()> {
        if self.debug {
            debug!(self.logger, "Updating fields";
                "sid" => self.sock_id,
                "fields" => format!("{:?}",fields),
            )
        }
        let sc = match self.sc {
            Some(ref s) => s,
            None => {
                raise!(
                    ReferenceError,
                    "Cannot update field: no datapath program installed yet!"
                );
            }
        };

        let ret = self.backend.update_field(sc, &get_fields(fields)[..]);

        match ret {
            Ok(()) => Ok(()),
            Err(e) => raise!(Exception, format!("Failed to update fields, err: {:?}", e)),
        }
    }

    fn set_program(
        &mut self,
        py: Python,
        program_name: &'static str,
        fields: Option<&PyList>,
    ) -> PyResult<()> {
        if self.debug {
            debug!(self.logger, "switching datapath programs";
                "sid" => self.sock_id,
                "program_name" => program_name.clone(),
            )
        }

        let ret = self.backend.set_program(
            program_name,
            fields
                .map(|list| get_fields(list))
                .as_ref()
                .map(|x| x.as_slice()),
        );

        match ret {
            Ok(sc) => {
                self.sc = Some(Rc::new(sc));
                Ok(())
            }
            Err(e) => raise!(
                Exception,
                format!("Failed to set datapath program: {:?}", e)
            ),
        }
    }
}

#[py::modinit(pyportus)]
fn init_mod(py: pyo3::Python<'static>, m: &PyModule) -> PyResult<()> {
    #[pyfn(m, "_connect")]
    fn _py_connect(
        py: pyo3::Python<'static>,
        ipc_str: String,
        alg: PyObject,
        debug: bool,
    ) -> PyResult<i32> {
        simple_signal::set_handler(&[Signal::Int, Signal::Term], move |_signals| {
            ::std::process::exit(1);
        });
        py_connect(py, ipc_str, alg, debug)
    }

    #[pyfn(m, "_try_compile")]
    fn _py_try_compile(py: pyo3::Python<'static>, prog: String) -> PyResult<String> {
        py_try_compile(py, prog)
    }

    m.add_class::<DatapathInfo>()?;
    m.add_class::<PyDatapath>()?;
    m.add_class::<PyReport>()?;

    Ok(())
}

use portus::lang;
use std::error::Error;
fn py_try_compile(_py: pyo3::Python<'static>, prog: String) -> PyResult<String> {
    match lang::compile(prog.as_bytes(), &[]) {
        Ok(_) => Ok("".to_string()),
        Err(e) => Ok(e.description().to_string()),
    }
}

fn py_connect(py: pyo3::Python<'static>, ipc: String, alg: PyObject, debug: bool) -> PyResult<i32> {
    let log = portus::algs::make_logger();

    // Check args
    if let Err(e) = portus::algs::ipc_valid(ipc.clone()) {
        raise!(ValueError, e);
    };

    let py_cong_alg = PyCongAlg {
        py,
        logger: log.clone(),
        alg_obj: alg,
        debug,
    };

    match ipc.as_str() {
        "unix" => {
            use portus::ipc::unix::Socket;
            let b = Socket::<ipc::Blocking>::new("portus")
                .map(|sk| BackendBuilder { sock: sk })
                .expect("create unix socket");
            portus::run::<_, PyCongAlg>(b, portus::Config { logger: Some(log) }, py_cong_alg)
                .unwrap();
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use portus::ipc::netlink::Socket;
            let b = Socket::<ipc::Blocking>::new()
                .map(|sk| BackendBuilder { sock: sk })
                .expect("create netlink socket");

            portus::run::<_, PyCongAlg>(b, portus::Config { logger: Some(log) }, py_cong_alg)
                .unwrap();
        }
        _ => unreachable!(),
    }
}

use std::os::raw::c_int;
// Creates an instance of cls and calls __init__(self, *args, **kwargs)
// Returns a pointer to the instance
fn _py_new_instance(
    py: Python,
    cls: *mut pyo3::ffi::PyTypeObject,
    args: *mut pyo3::ffi::PyObject,
    kwargs: *mut pyo3::ffi::PyObject,
) -> PyResult<PyObject> {
    unsafe {
        match (*cls).tp_new {
            Some(tp_new) => {
                let obj =
                    pyo3::PyObject::from_owned_ptr(py, tp_new(cls, py_none!(py), py_none!(py)));
                match (*cls).tp_init {
                    Some(tp_init) => {
                        let p = (&obj).into_ptr();
                        let ret: c_int = tp_init(p, args, kwargs);
                        // If there's an error in init, print the traceback
                        if ret < 0 {
                            pyo3::ffi::PyErr_PrintEx(0);
                        }
                        Ok(obj)
                    }
                    None => Ok(py.None()),
                }
            }
            None => Ok(py.None()),
        }
    }
}

pub fn py_setattr<N, V>(o: &PyObject, py: Python, attr_name: N, val: V) -> PyResult<()>
where
    N: ToBorrowedObject,
    V: ToBorrowedObject,
{
    attr_name.with_borrowed_ptr(py, move |attr_name| {
        val.with_borrowed_ptr(py, |val| unsafe {
            let ret = pyo3::ffi::PyObject_SetAttr(o.as_ptr(), attr_name, val);
            if ret != -1 {
                Ok(())
            } else {
                Err(PyErr::fetch(py))
            }
        })
    })
}
