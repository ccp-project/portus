#![feature(box_patterns, specialization, const_fn)]
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

    fn init_programs(cfg: Config<T, PyAlg>) -> Vec<(String, String)> {
        let py = cfg.config.py;

        let py_alg_inst =
            py_create_flow(py, cfg.config.alg_class).unwrap_or_else(|e| {
                e.print(py); panic!("Failed to instantiate python class")
            });

        let programs = match py_alg_inst.call_method0(py, "init_programs") {
            Ok(ret) => {
                let list : &PyList = match ret.extract(py) {
                    Ok(l) => l,
                    Err(e) => {
                        e.print(py);
                        panic!("init_programs() must return a *list* of tuples of (2) strings.\nreturn value was not a list.")
                    }
                };
                let programs : Vec<(String, String)> = list.iter().map(|tuple_obj| {
                    let tuple : &PyTuple = match tuple_obj.extract() {
                        Ok(t) => t,
                        Err(e) => {
                            e.print(py);
                            panic!("init_programs() must return a list of *tuples* of (2) strings.\ngot a list, but the elements were not tuples.")
                        }
                    };
                    if tuple.len() != 2 {
                        panic!(format!("init_programs() must return a list of tuples of *(2)* strings.\ngot a list of tuples, but a tuple had {} elements, not 2.", tuple.len()));
                    }
                    let program_name = match PyString::try_from(tuple.get_item(0)) {
                        Ok(pn) => pn.to_string_lossy().into_owned(),
                        Err(_) => {
                            panic!("init_programs() must return a list of tuples of (2) *strings*.\ngot a list of tuples, but the first element was not a string.")
                        }
                    };
                    let program_string = match PyString::try_from(tuple.get_item(1)) {
                        Ok(ps) => ps.to_string_lossy().into_owned(),
                        Err(_) => {
                            panic!("init_programs() must return a list of tuples of (2) *strings*.\ngot a list of tuples, but the second element was not a string.")
                        }
                    };
                    (program_name, program_string)
                }).collect::<Vec<(String, String)>>();
                programs
            }
            Err(e) => {
                e.print(py);
                panic!("error calling init_programs()");
            }
        };

        // TODO figure out how to deallocate and clean up instance
        programs
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
                if m.program_uid != s.program_uid {
                    if self.config.debug {
                        self.logger.as_ref().map(|log| {
                            debug!(log, "Report is stale, ignoring...";
                               "sid"        => sock_id,
                               "report_uid" => m.program_uid,
                               "scope_uid"  => s.program_uid,
                           )
                        });
                    }
                    return;
                }
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
    // TODO implement close, deallocate memory from class
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
            Ok(val)               => Ok(val),
            Err(portus::Error(e)) => raise!(Exception, format!("Failed to get {}: {}", name, e)),
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

    fn update_fields(&self, py : Python, fields : &PyList) -> PyResult<()> {
        if self.debug {
            self.logger.as_ref().map(|log| {
                debug!(log, "Updating fields";
                   "sid" => self.sock_id,
                   "fields" => format!("{:?}",fields), 
               )
            });
        }
        let sc = match self.sc {
            Some(ref s) => { s }
            None => {
                raise!(ReferenceError, "Cannot update field: no datapath program installed yet!");
            }
        };

        let ret = {
            let items : Vec<(String,u32)> = fields.into_iter().map(|tuple_ref| {
                let tuple_obj : PyObject = tuple_ref.into();
                let tuple:&PyTuple = match tuple_obj.extract(py) {
                    Ok(t) => t,
                    Err(_) => {
                        raise!(TypeError, "second argument to datapath.update_fields must be a list of tuples")
                    }
                };
                if tuple.len() != 2 {
                    raise!(TypeError, "second argument to datapath.update_fields must be a list of tuples with exactly two values each");
                }
                let name = match PyString::try_from(tuple.get_item(0)) {
                    Ok(ps) => ps.to_string_lossy().into_owned(),
                    Err(_) => raise!(TypeError, "second argument to datapath.update_fields must be a list of tuples of the form (string, int|float)"),
                };
                let val = match tuple.get_item(1).extract::<u32>() {
                    Ok(v) => v,
                    Err(_) => raise!(TypeError, "second argument to datapath.update_fields must be a list of tuples of the form (string, int|float)")
                };
                Ok((name,val))
            }).collect::<Result<Vec<(String, u32)>, _>>().unwrap();

            let args: Vec<(&str,u32)> = items.iter().map(|(s,i)| (&s[..],i.clone())).collect();
            self.backend.update_field(sc, &args[..])
        };

        match ret {
            Ok(()) => Ok(()),
            Err(e) => { raise!(Exception, format!("Failed to update fields, err: {:?}", e)) },
        }
        
    }

    fn set_program(&mut self, py : Python, program_name: String, fields : Option<&PyList>) -> PyResult<()> {
        if self.debug {
            self.logger.as_ref().map(|log| {
                debug!(log, "switching datapath programs";
                   "sid" => self.sock_id,
                   "program_name" => program_name.clone(),
               )
            });
        }

        let ret : Result<Scope, _> = match fields {
            Some(list) => {
                let items : Vec<(String,u32)> = list.into_iter().map(|tuple_ref| {
                    let tuple_obj : PyObject = tuple_ref.into();
                    let tuple:&PyTuple = match tuple_obj.extract(py) {
                        Ok(t) => t,
                        Err(_) => {
                            raise!(TypeError, "second argument to datapath.set_program must be a list of tuples")
                        }
                    };
                    if tuple.len() != 2 {
                        raise!(TypeError, "second argument to datapath.set_program must be a list of tuples with exactly two values each");
                    }
                    let name = match PyString::try_from(tuple.get_item(0)) {
                        Ok(ps) => ps.to_string_lossy().into_owned(),
                        Err(_) => raise!(TypeError, "second argument to datapath.set_program must be a list of tuples of the form (string, int|float)"),
                    };
                    let val = match tuple.get_item(1).extract::<u32>() {
                        Ok(v) => v,
                        Err(_) => raise!(TypeError, "second argument to datapath.set_program must be a list of tuples of the form (string, int|float)")
                    };
                    Ok((name,val))
                }).collect::<Result<Vec<(String, u32)>, _>>().unwrap();

                let args: Vec<(&str, u32)> = items.iter().map(|(s,i)| (&s[..],i.clone())).collect();
                self.backend.set_program(program_name, Some(&args[..]))
            }
            None => {
                self.backend.set_program(program_name, None)
            }
        };

        match ret {
            Ok(sc) => {
                self.sc = Some(Rc::new(sc));
                Ok(())
            }
            Err(e) => {
                raise!(Exception, format!("Failed to set datapath program: {:?}", e))
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

    #[pyfn(m, "_try_compile")]
    fn _py_try_compile(py:pyo3::Python<'static>, prog:String) -> PyResult<String> {
        py_try_compile(py, prog)
    }

    m.add_class::<DatapathInfo>()?;
    m.add_class::<PyDatapath>()?;
    m.add_class::<PyReport>()?;

    Ok(())
}


use portus::lang;
use std::error::Error;
fn py_try_compile(_py:pyo3::Python<'static>, prog:String) -> PyResult<String> {
    match lang::compile(prog.as_bytes(), &[]) {
        Ok(_)  => Ok("".to_string()),
        Err(e) => Ok(e.description().to_string()),
    }
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
