//! This binary runs integration tests for libccp and portus.
//! To add an integration test:
//!     1. Add a name for the integration test at the top of this file
//!     2. Add a struct, Impl, and CongAlg trait Impl for the test in src/lib.rs.
//!     3. In "run_test", add a case corresponding to running the test
//!     4. Finally call run test for this added test.

#[macro_use]
extern crate slog;
extern crate libccp_integration_test;
extern crate portus;

use libccp_integration_test::{TestBasicSerialize, TestTiming, TestUpdateFields, TestVolatileVars};
use std::process::{Command, Stdio};
use portus::ipc::{BackendBuilder, Blocking};
use portus::ipc::unix::Socket;
use std::{thread,time};
use std::sync::mpsc;
use std::env;

#[derive(Debug)]
enum Test {
    TestBasicSerialize,
    TestTiming,
    TestUpdateFields,
    TestVolatileVars
}

// Spawn userspace ccp
fn start_ccp<T>(log: slog::Logger, tx: mpsc::Sender<String>, test: Test) -> portus::CCPHandle
    where T: portus::CongAlg<
        Socket<Blocking>,
        Config=libccp_integration_test::IntegrationTestConfig,
    > + 'static
{
    let cfg = libccp_integration_test::IntegrationTestConfig{
        sender: tx, // used for the algorithm to send a signal whent the tests are over
    };
    
	info!(log, "Start portus-libccp integration test: {:?}", test);
    let b = Socket::<Blocking>::new("in", "out")
        .map(|sk| BackendBuilder {
            sock: sk,
        })
        .expect("ipc initialization");
    portus::spawn::<_, T>(
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

// Runs a specific intergration test
fn run_test(libccp_location: String, log: slog::Logger, test: Test) {
    let (tx, rx) = mpsc::channel();
    let ccp_handle: portus::CCPHandle = match test {
        Test::TestBasicSerialize => start_ccp::<TestBasicSerialize<Socket<Blocking>>>(log, tx, test),
        Test::TestTiming => start_ccp::<TestTiming<Socket<Blocking>>>(log, tx, test),
        Test::TestUpdateFields  => start_ccp::<TestUpdateFields<Socket<Blocking>>>(log, tx, test),
        Test::TestVolatileVars => start_ccp::<TestVolatileVars<Socket<Blocking>>>(log, tx, test),
    };

    // sleep before spawning mock datapath, so sockets can be setup properly
    thread::sleep(time::Duration::from_millis(500));

    // spawn libccp
    let mut libccp_process = start_libccp(libccp_location);
    // wait for program to finish
    let wait_for_done = thread::spawn(move ||{
        let msg = rx.recv().unwrap();
        assert!(msg == libccp_integration_test::DONE, "Received wrong message on channel");
        ccp_handle.kill(); // causes backend to stop iterating
        libccp_process.kill().unwrap();
        ccp_handle.wait().unwrap();
    });
    
    wait_for_done.join().unwrap();
}

fn main() {
    let args: Vec<String> = env::args().collect(); // expect that libccp is in args[1]
    let libccp_location = &args[1];
    let log = portus::algs::make_logger();

    // run test with various tests
    run_test(libccp_location.to_string(), log.clone(), Test::TestBasicSerialize);
    run_test(libccp_location.to_string(), log.clone(), Test::TestTiming);
    run_test(libccp_location.to_string(), log.clone(), Test::TestUpdateFields);
    run_test(libccp_location.to_string(), log.clone(), Test::TestVolatileVars);
    info!(log, "Passed all integration tests!";);
}
