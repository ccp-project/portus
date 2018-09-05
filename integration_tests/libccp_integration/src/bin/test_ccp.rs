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

use std::sync::mpsc;
use std::thread;

use portus::ipc::{BackendBuilder, Blocking};
use portus::ipc::unix::Socket;

use libccp_integration_test::scenarios::{TestBasicSerialize, TestTiming, TestUpdateFields, TestVolatileVars, TestPresetVars};

#[derive(Debug)]
enum Test {
    TestBasicSerialize,
    TestTiming,
    TestUpdateFields,
    TestVolatileVars,
    TestPresetVars,
}

// Spawn userspace ccp
fn start_ccp<T>(log: slog::Logger, tx: mpsc::Sender<String>, test: Test) -> portus::CCPHandle
    where T: portus::CongAlg<
        Socket<Blocking>,
        Config=libccp_integration_test::scenarios::IntegrationTestConfig,
    > + 'static
{
    let cfg = libccp_integration_test::scenarios::IntegrationTestConfig{
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

// Runs a specific intergration test
fn run_test(log: slog::Logger, test: Test) {
    let (tx, rx) = mpsc::channel();

    // spawn libccp
    let (mock_dp_ready_tx, mock_dp_ready_rx) = mpsc::channel();
    let (mock_dp_done_tx, mock_dp_done_rx) = mpsc::channel();
    let dp_log = log.clone();
    thread::spawn(move || {
        use libccp_integration_test::mock_datapath;
        mock_datapath::start(mock_dp_done_rx, mock_dp_ready_tx, dp_log);
    });

    // wait for mock datapath to spawn
    mock_dp_ready_rx.recv().unwrap();
    let ccp_handle: portus::CCPHandle = match test {
        Test::TestBasicSerialize => start_ccp::<TestBasicSerialize<Socket<Blocking>>>(log.clone(), tx, test),
        Test::TestTiming => start_ccp::<TestTiming<Socket<Blocking>>>(log.clone(), tx, test),
        Test::TestUpdateFields  => start_ccp::<TestUpdateFields<Socket<Blocking>>>(log.clone(), tx, test),
        Test::TestVolatileVars => start_ccp::<TestVolatileVars<Socket<Blocking>>>(log.clone(), tx, test),
        Test::TestPresetVars => start_ccp::<TestPresetVars<Socket<Blocking>>>(log.clone(), tx, test)
    };

    // wait for program to finish
    let wait_for_done = thread::spawn(move ||{
        let msg = rx.recv().unwrap();
        assert!(msg == libccp_integration_test::scenarios::DONE, "Received wrong message on channel");
        ccp_handle.kill(); // causes backend to stop iterating
        mock_dp_done_tx.send(()).unwrap();
        ccp_handle.wait().unwrap();
    });
    
    wait_for_done.join().unwrap();
}

fn main() {
    let log = portus::algs::make_logger();

    // run test with various tests
    run_test(log.clone(), Test::TestBasicSerialize);
    run_test(log.clone(), Test::TestTiming);
    run_test(log.clone(), Test::TestUpdateFields);
    run_test(log.clone(), Test::TestVolatileVars);
    run_test(log.clone(), Test::TestPresetVars);
    info!(log, "Passed all integration tests!";);
}
