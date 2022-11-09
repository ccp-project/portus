#![allow(non_snake_case)]

use portus::ipc;
use portus::ipc::BackendBuilder;
use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use pyo3::prelude::*;
use pyo3::types::*;
use pyo3::{exceptions, ToBorrowedObject};
use simple_signal::Signal;
use std::rc::{Rc, Weak};

#[macro_export]
macro_rules! raise {
    ($errtype:ident, $msg:expr) => {
        Err(PyErr::new::<exceptions::$errtype, _>($msg))
    };
}

macro_rules! py_none {
    ($py:expr) => {
        PyTuple::empty($py).into_ptr()
    };
}

mod cong_alg;
use cong_alg::*;

// Copy of the datapath class, $[pyo3(get)] necessary to access the fields in python
#[pyclass(weakref, dict)]
pub struct DatapathInfo {
    #[pyo3(get)]
    pub sock_id: u32,
    #[pyo3(get)]
    pub init_cwnd: u32,
    #[pyo3(get)]
    pub mss: u32,
    #[pyo3(get)]
    pub src_ip: u32,
    #[pyo3(get)]
    pub src_port: u32,
    #[pyo3(get)]
    pub dst_ip: u32,
    #[pyo3(get)]
    pub dst_port: u32,
}

#[pyclass(weakref, dict)]
pub struct Measurements {
    #[pyo3(get)]
    pub acked: u32,
    #[pyo3(get)]
    pub was_timeout: bool,
    #[pyo3(get)]
    pub sacked: u32,
    #[pyo3(get)]
    pub loss: u32,
    #[pyo3(get)]
    pub rtt: u32,
    #[pyo3(get)]
    pub inflight: u32,
}

/// Convenience wrapper around the Report struct for sending to python,
/// keeps a copy of the scope so the python user doesn't need to manage it
#[pyclass(weakref, dict, unsendable)]
struct PyReport {
    report: Report,
    sc: Weak<Scope>,
}

#[pyproto]
impl<'p> pyo3::class::PyObjectProtocol<'p> for PyReport {
    fn __getattr__(&'p self, name: String) -> PyResult<u64> {
        let field_name = match name.as_ref() {
            "Cwnd" | "Rate" => name.clone(),
            _ => format!("Report.{}", name),
        };
        let sc = match self.sc.upgrade() {
            Some(sc) => sc,
            None => {
                return raise!(
                    PyException,
                    format!(
                        "Failed to get {}: no datapath program installed",
                        field_name.clone()
                    )
                );
            }
        };
        match self.report.get_field(field_name.as_ref(), &sc) {
            Ok(val) => Ok(val),
            Err(portus::Error(e)) => raise!(PyException, format!("Failed to get {}: {}", name, e)),
        }
    }
}

fn get_fields(list: &PyList) -> Vec<(&str, u32)> {
    list.into_iter()
        .map(|tuple_ref| {
            tuple_ref.extract()
                .expect("second argument to datapath.set_program must be a list of tuples of the form (string, int)")
        })
    .collect::<_>()
}

/// Convenience wrapper around datapath struct,
/// python keeps a pointer to this for talking to the datapath
#[pyclass(weakref, dict, unsendable)]
struct PyDatapath {
    backend: Box<dyn DatapathTrait>,
    sc: Option<Rc<Scope>>,
    sock_id: u32,
}

#[pymethods]
impl PyDatapath {
    fn update_field(&self, _py: Python, reg_name: String, val: u32) -> PyResult<()> {
        tracing::debug!(sock_id = ?self.sock_id, ?reg_name, ?val, "Updating field");
        let sc = match self.sc {
            Some(ref s) => s,
            None => {
                return raise!(
                    PyReferenceError,
                    "Cannot update field: no datapath program installed yet!"
                );
            }
        };
        match self.backend.update_field(sc, &[(reg_name.as_str(), val)]) {
            Ok(()) => Ok(()),
            Err(e) => raise!(PyException, format!("Failed to update field, err: {:?}", e)),
        }
    }

    fn update_fields(&self, _py: Python, fields: &PyList) -> PyResult<()> {
        tracing::debug!(sock_id = ?self.sock_id, ?fields, "Updating fields");
        let sc = match self.sc {
            Some(ref s) => s,
            None => {
                return raise!(
                    PyReferenceError,
                    "Cannot update field: no datapath program installed yet!"
                );
            }
        };

        let ret = self.backend.update_field(sc, &get_fields(fields)[..]);

        match ret {
            Ok(()) => Ok(()),
            Err(e) => raise!(
                PyException,
                format!("Failed to update fields, err: {:?}", e)
            ),
        }
    }

    fn set_program(
        &mut self,
        _py: Python,
        program_name: &str,
        fields: Option<&PyList>,
    ) -> PyResult<()> {
        tracing::debug!(sock_id = ?self.sock_id, ?program_name, "switching datapath programs");

        // we have a &'py str and need a &'static str.
        //
        // SAFETY: *in practice*, portus does not rely on the 'static lifetime of program_name (it
        // does not keep it around, only uses it to lookup in its HashMap); therefore, the lifetime of
        // `program_name`, `'py`, is sufficient.
        // If `backend.set_program` changes to keep the string around, then this will no longer
        // work.
        let pname: &'static str = unsafe { std::mem::transmute(program_name) };

        let ret = self.backend.set_program(
            pname,
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
                PyException,
                format!("Failed to set datapath program: {:?}", e)
            ),
        }
    }
}

#[pymodule]
fn pyportus(_py: Python, m: &PyModule) -> PyResult<()> {
    #[pyfn(m)]
    fn start_inner(py: Python, ipc_str: String, alg: PyObject) -> PyResult<i32> {
        simple_signal::set_handler(&[Signal::Int, Signal::Term], move |_signals| {
            tracing::info!("exiting");
            ::std::process::exit(1);
        });

        py_start_inner(py, ipc_str, alg)
    }

    #[pyfn(m)]
    fn try_compile(py: Python, prog: String) -> PyResult<String> {
        py_try_compile(py, prog)
    }

    m.add_class::<DatapathInfo>()?;
    m.add_class::<PyDatapath>()?;
    m.add_class::<PyReport>()?;
    Ok(())
}

fn py_start_inner<'p>(py: Python<'p>, ipc: String, alg: PyObject) -> PyResult<i32> {
    // Check args
    if let Err(e) = portus::algs::ipc_valid(ipc.clone()) {
        return raise!(PyValueError, e);
    };

    tracing_subscriber::fmt::init();

    let py_cong_alg = PyCongAlg { py, alg_obj: alg };

    // SAFETY: _connect will block the Python program, so really we will hold the GIL for
    // the remainder of the program's lifetime, which is 'static.
    let py_cong_alg: PyCongAlg<'static> = unsafe { std::mem::transmute(py_cong_alg) };
    tracing::info!(?ipc, "starting CCP");
    match ipc.as_str() {
        "unix" => {
            use ipc::unix::Socket;
            let b = Socket::<ipc::Blocking>::new("portus")
                .map(|sk| BackendBuilder { sock: sk })
                .expect("create unix socket");
            portus::RunBuilder::new(b).default_alg(py_cong_alg).run()
        }
        #[cfg(all(target_os = "linux"))]
        "netlink" => {
            use ipc::netlink::Socket;
            let b = Socket::<ipc::Blocking>::new()
                .map(|sk| BackendBuilder { sock: sk })
                .expect("create netlink socket");
            portus::RunBuilder::new(b).default_alg(py_cong_alg).run()
        }
        _ => unreachable!(),
    }
    .or_else(|e| raise!(PyException, format!("{:?}", e)))?;
    Ok(0)
}

fn py_try_compile<'p>(_py: Python<'p>, prog: String) -> PyResult<String> {
    use portus::lang;
    match lang::compile(prog.as_bytes(), &[]) {
        Ok(_) => Ok("".to_string()),
        Err(e) => Ok(format!("{}", e)),
    }
}

// Creates an instance of cls and calls __init__(self, *args, **kwargs)
// Returns a pointer to the instance
fn _py_new_instance(
    py: Python,
    cls: *mut pyo3::ffi::PyTypeObject,
    args: *mut pyo3::ffi::PyObject,
    kwargs: *mut pyo3::ffi::PyObject,
) -> PyResult<PyObject> {
    use std::os::raw::c_int;
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
    use pyo3::AsPyPointer;
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
