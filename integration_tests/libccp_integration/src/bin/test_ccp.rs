#[macro_use]
extern crate slog;
extern crate libccp_integration_test;
extern crate portus;

use libccp_integration_test::IntegrationTest;
use std::process::{Command, Stdio};
use portus::ipc::{BackendBuilder, Blocking};
use std::{thread,time};
use std::sync::mpsc;
use std::env;

fn make_args(tx: mpsc::Sender<String>) -> Result<(libccp_integration_test::IntegrationTestConfig), String> {
	Ok(
		libccp_integration_test::IntegrationTestConfig {
		    sender: tx, // used for the algorithm to send a signal whent the tests are over
		}
    )
}

// Spawn userspace ccp
fn start_ccp(log: slog::Logger, tx: mpsc::Sender<String>) -> portus::CCPHandle {
    let cfg = make_args(tx)
		.map_err(|e| warn!(log, "bad argument"; "err" => ?e))
		.unwrap();
    
	info!(log, "Start ccp integration test";);
	use portus::ipc::unix::Socket;
    let b = Socket::<Blocking>::new("in", "out")
        .map(|sk| BackendBuilder {
            sock: sk,
        })
        .expect("ipc initialization");
	portus::spawn::<_, IntegrationTest<_>>(
		b,
		portus::Config {
			logger: Some(log),
			config: cfg,
		}
	)
}

// Thread to spawn libccp, returns pid
fn start_libccp(libccp_location: String) -> std::process::Child {
    Command::new(libccp_location)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn().unwrap()
}

fn main() {
	let args: Vec<String> = env::args().collect(); // expect that libccp is in args[1]
    let libccp_loc = &args[1];
    let log = portus::algs::make_logger();
    let (tx, rx) = mpsc::channel();
    let ccp_handle = start_ccp(log, tx); 
    
    // sleep before spawning mock datapath, so sockets can be setup properly
    thread::sleep(time::Duration::from_millis(500));

    // spawn libccp
    let mut libccp_process = start_libccp(libccp_loc.to_string());

    // wait for program to finish
    let wait_for_done = thread::spawn(move ||{
        let msg = rx.recv().unwrap();
        assert!(msg == String::from("Done!"), "Received wrong message on channel");
        ccp_handle.kill(); // causes backend to stop iterating
        libccp_process.kill().unwrap();
        ccp_handle.wait().unwrap();
    });
    
    wait_for_done.join().unwrap();
}
