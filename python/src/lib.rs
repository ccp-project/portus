#![feature(box_patterns, proc_macro, specialization, const_fn)]
use std::rc::{Rc,Weak};

#[macro_use]
extern crate slog;
extern crate time;

extern crate portus;
use portus::{CongAlg, Config, Datapath, DatapathTrait, Report};
use portus::ipc;
use portus::ipc::{BackendBuilder, Ipc};
use portus::lang::Scope;

extern crate pyo3;
use pyo3::prelude::*;


macro_rules! raise {
    ($errtype:ident, $msg:expr) => (
        return Err(PyErr::new::<exc::$errtype, _>($msg));
    );
    ($errtype:ident, $msg:expr, false) => (
        Err(PyErr::new::<exc::$errtype, _>($msg));
    );
}


pub struct PyAlg {
    logger          : Option<slog::Logger>,
    config          : PyAlgConfig,
    // Instance of the flow in python
    py_alg_inst     : PyObject,
}

#[derive(Clone, Copy)]
pub struct PyAlgConfig {
    py        : Python<'static>,
    alg_class : *mut pyo3::ffi::PyTypeObject, 
    debug     : bool,
}

// Convenience wrapper around datapath struct,
// python keeps a pointer to this for talking to the datapath
#[py::class(gc,weakref,dict)]
struct PyDatapath {
    backend : Box<DatapathTrait>,
    sc      : Option<Rc<Scope>>,
    logger  : Option<slog::Logger>,
    debug   : bool,
    sock_id : u32,
}

// Copy of the datapath class, $[prop(get)] necessary to access the fields in python
#[py::class(gc,weakref,dict)]
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

// Convenience wrapper around the Report struct for sending to python,
// keeps a copy of the scope so the python user doesn't need to manage it 
#[py::class(gc,weakref,dict)]
struct PyReport {
    report : Report,
    sc     : Weak<Scope>,
}



impl<T: Ipc> CongAlg<T> for PyAlg {
    type Config = PyAlgConfig;

    fn name() -> String {
        // TODO if we want the actual name, need to re-define name() to take &self
        String::from("python")
    }

    fn create(control:Datapath<T>, cfg:Config<T, PyAlg>, info:portus::DatapathInfo) -> Self {
        let py = cfg.config.py;

        if cfg.config.debug {
            cfg.logger.as_ref().map(|log| {
                debug!(log, "New flow"; "sid" => control.get_sock_id()) 
            });
        };

        let py_datapath = py.init(|_| PyDatapath {
            sock_id : control.get_sock_id(),
            backend : Box::new(control),
            logger  : cfg.logger.clone(),
            sc      : Default::default(),
            debug   : cfg.config.debug,
        }).unwrap_or_else(|e| {
            e.print(py); panic!("Failed to create PyDatapath")
        });

        let py_info = py.init(|_| DatapathInfo {
            sock_id   : info.sock_id,
            init_cwnd : info.init_cwnd,
            mss       : info.mss,
            src_ip    : info.src_ip,
            src_port  : info.src_port,
            dst_ip    : info.dst_ip,
            dst_port  : info.dst_port,
        }).unwrap_or_else(|e| {
            e.print(py); panic!("Faile to create DatapathInfo")
        });

        let py_alg_inst = py_create_flow(
            cfg.config.py, 
            cfg.config.alg_class,
        ).unwrap_or_else(|e| {
            e.print(py); panic!("Failed to instantiate python class")
        });

        py_setattr(&py_alg_inst, py, "datapath", py_datapath).unwrap_or_else(|e| {
        	e.print(py); panic!("Failed to set alg.dapath")
        });
        py_setattr(&py_alg_inst, py, "datapath_info", py_info).unwrap_or_else(|e| {
        	e.print(py); panic!("Failed to set alg.datapath_info")
        });

        match py_alg_inst.call_method0(py, "on_create") {
            Ok(_ret) => {}
            Err(e) => {
                e.print(py);
                cfg.logger.as_ref().map(|log| {
                    error!(log, "on_create() failed to complete";
                       "sid" => info.sock_id,
                   );
                });
            }
        };


        Self {
            logger : cfg.logger,
            config : cfg.config,
            py_alg_inst,
        }
    }

    fn on_report(&mut self, sock_id:u32, m:Report) {
        let py = self.config.py;
        
        if self.config.debug {
            self.logger.as_ref().map(|log| {
                debug!(log, "Got report";
                   "sid" => sock_id,
               )
            });
        }

        let pyd_obj : PyObject = match self.py_alg_inst.getattr(py, "datapath") {
            Ok(o)  => o,
            Err(e) => { e.print(py); panic!("Failed to dereference flow.datapath") }
        };
        let pyd : Py<PyDatapath> = unsafe {
            pyo3::Py::from_borrowed_ptr(pyd_obj.as_ptr())
        };
        let pyd_ref : &PyDatapath = pyd.as_ref(py);
        let report = match pyd_ref.sc {
            Some(ref s) => {
                let rep = py.init(|_t| PyReport {
                    report: m,
                    sc: Rc::downgrade(s),
                }).unwrap_or_else(|e| {
                    e.print(py);
                    panic!("Failed to create PyReport")
                });
                rep
            }
            None => {
                self.logger.as_ref().map(|log| {
                    error!(log, "Failed to get report: can't find scope (no datapath program installed yet";
                       "sid" => sock_id,
                   );
                });
                return;
            }
        };
        //let args = PyTuple::new(py, &[m.fields]);
        let args = PyTuple::new(py, &[report]);
        match self.py_alg_inst.call_method1(py, "on_report", args) {
            Ok(_ret) => {}
            Err(e) => {
                e.print(py);
                self.logger.as_ref().map(|log| {
                    error!(log, "on_report() failed to complete";
                       "sid" => sock_id,
                   );
                });
            }
        };
    }
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
                raise!(Exception, format!("Failed to get {}: no datapath program installed", field_name.clone()));
            }
        };
        match self.report.get_field(field_name.as_ref(), &sc) {
            Some(val) => { Ok(val) }
            None => {
                raise!(AttributeError, format!("Report has no variable {}", name));
            }
        }
    }
}

#[py::methods]
impl PyDatapath {

    fn update_field(&self, _py : Python, reg_name : String, val : u32) -> PyResult<()> {
        if self.debug {
            self.logger.as_ref().map(|log| {
                debug!(log, "Updating field";
                   "sid" => self.sock_id,
                   "field" => reg_name.clone(),
                   "val" => val,
               )
            });
        }
        let sc = match self.sc {
            Some(ref s) => { s }
            None => {
                raise!(ReferenceError, "Cannot update field: no datapath program installed yet!");
            }
        };
        match self.backend.update_field(sc, &[(reg_name.as_str(), val)]) {
            Ok(()) => Ok(()),
            Err(e) => { raise!(Exception, format!("Failed to update field, err: {:?}", e)) }
        }
    }

    fn install(&mut self, _py : Python, prog: String) -> PyResult<()> {
        if self.debug {
            self.logger.as_ref().map(|log| {
                debug!(log, "Installing new datapath program";
                   "sid" => self.sock_id,
               )
            });
        }

        match self.backend.install(prog.as_bytes()) {
            Ok(sc) => {
                self.sc = Some(Rc::new(sc));
                Ok(())
            }
            Err(e) => {
                raise!(Exception, format!("Failed to install datapath program: {:?}", e))
            }
        }

    }
}


#[py::modinit(pyportus)]
fn init_mod(py:pyo3::Python<'static>, m:&PyModule) -> PyResult<()> {

    #[pyfn(m, "_connect")]
    fn _py_connect(py:pyo3::Python<'static>, ipc_str:String, alg:&PyObjectRef, blocking:bool, debug:bool) -> PyResult<i32> {
        py_connect(py, ipc_str, alg, blocking, debug)
    }

    m.add_class::<DatapathInfo>()?;
    m.add_class::<PyDatapath>()?;
    m.add_class::<PyReport>()?;

    Ok(())
}

use std::ffi::CStr;
fn py_connect(py:pyo3::Python<'static>, ipc:String, alg:&PyObjectRef, blocking:bool, debug:bool) -> PyResult<i32> {

    let log = portus::algs::make_logger();

    // Check args
    if let Err(e) = portus::algs::ipc_valid(ipc.clone()) {
        raise!(ValueError, e);
    };

    // Obtain pointer to class object that can be instantiated
    let alg:PyObject = alg.into();
    let alg_type:&PyType = match alg.extract(py) {
        Ok(t)   => t,
        Err(_) => { 
            raise!(TypeError, "'alg' argument must be of type 'type'. For example: class Alg(object): .. "); 
        }
    };
    let alg_class:*mut pyo3::ffi::PyTypeObject = unsafe { 
        alg_type.as_type_ptr() 
    }; 

    // equivalent to "class.__name__"
    let alg_name = unsafe { CStr::from_ptr((*alg_class).tp_name).to_string_lossy().into_owned() };

    info!(log, "Starting CCP";
        "algorithm" => format!("python.{}", alg_name),
        "ipc" => ipc.clone(),
        "debug" => debug.clone(),
        "blocking" => blocking.clone(),
    );

    let cfg = PyAlgConfig {
        py,
        alg_class,
        debug,
    };

    match blocking {
	    true => match ipc.as_str() {

	        "unix" => {
	            use portus::ipc::unix::Socket;
	            let b = Socket::<ipc::Blocking>::new("in", "out")
	                .map(|sk| BackendBuilder {sock: sk})
	                .expect("create unix socket");
	            portus::run::<_, PyAlg>(
	                b,
	                &portus::Config {
	                    logger: Some(log),
	                    config: cfg, 
	                }
	            ).unwrap();
	        }

	        #[cfg(all(target_os = "linux"))]
	        "netlink" => {
	            use portus::ipc::netlink::Socket;
	            let b = Socket::<ipc::Blocking>::new()
	                .map(|sk| BackendBuilder {sock: sk})
	                .expect("create netlink socket");

	            portus::run::<_, PyAlg>(
	                b,
	                &portus::Config {
	                    logger: Some(log),
	                    config: cfg,
	                }
	            ).unwrap();

	        }

	        _ => unreachable!()

	    }
	    false => match ipc.as_str() {

	        "unix" => {
	            use portus::ipc::unix::Socket;
	            let b = Socket::<ipc::Nonblocking>::new("in", "out")
	                .map(|sk| BackendBuilder {sock: sk})
	                .expect("create unix socket");
	            portus::run::<_, PyAlg>(
	                b,
	                &portus::Config {
	                    logger: Some(log),
	                    config: cfg, 
	                }
	            ).unwrap();
	        }

	        #[cfg(all(target_os = "linux"))]
	        "netlink" => {
	            use portus::ipc::netlink::Socket;
	            let b = Socket::<ipc::Nonblocking>::new()
	                .map(|sk| BackendBuilder {sock: sk})
	                .expect("create netlink socket");

	            portus::run::<_, PyAlg>(
	                b,
	                &portus::Config {
	                    logger: Some(log),
	                    config: cfg,
	                }
	            ).unwrap();

	        }

	        _ => unreachable!()

	    }
    }
}

use std::os::raw::c_int;
// Creates an instance of cls and calls __init__(self, datapath, info)
// Returns a pointer to the instance
fn py_create_flow(py : Python, cls :*mut pyo3::ffi::PyTypeObject) -> PyResult<PyObject> {
    let args = PyTuple::empty(py).into_ptr(); 
    let kwargs = PyTuple::empty(py).into_ptr();
    unsafe {
        match (*cls).tp_new {
            Some(tp_new) => {
                let obj = pyo3::PyObject::from_owned_ptr(py, tp_new(cls, args, kwargs));
                match (*cls).tp_init {
                    Some(tp_init) => {
                        let p = (&obj).into_ptr();
                        let ret : c_int = tp_init(p, args, kwargs);
                        // If there's an error in init, print the traceback
                        if ret < 0 {
                            pyo3::ffi::PyErr_PrintEx(0);
                        }
                        Ok(obj)
                    }
                    None => Ok(py.None())
                }
            }
            None => Ok(py.None())
        }
    }
}

pub fn py_setattr<N, V>(o : &PyObject, py : Python, attr_name : N, val : V) -> PyResult<()>
	where N: ToBorrowedObject, V: ToBorrowedObject
{
	attr_name.with_borrowed_ptr(
		py, move |attr_name|
		val.with_borrowed_ptr(py, |val| unsafe {
			let ret = pyo3::ffi::PyObject_SetAttr(o.as_ptr(), attr_name, val);
			if ret != -1 {
				Ok(())
			} else {
				Err(PyErr::fetch(py))
			}
		})
	)
}
