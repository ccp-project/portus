use std::marker::PhantomData;
use std::sync::mpsc;

use portus::ipc::Ipc;
use portus::lang::Scope;
use portus::{CongAlg, Datapath, DatapathInfo, DatapathTrait, Flow, Report};
use std::collections::HashMap;

pub const ACKED_PRIMITIVE: u32 = 5; // libccp uses this same value for acked_bytes

mod mock_datapath;

pub trait IntegrationTest: Sized {
    fn new() -> Self;
    fn datapath_programs() -> HashMap<&'static str, String>;
    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope>;
    fn check_test(&mut self, sc: &Scope, t: std::time::Instant, sock_id: u32, m: &Report) -> bool;
}

pub struct TestBaseConfig<T: IntegrationTest>(mpsc::Sender<Result<(), ()>>, PhantomData<T>);

impl<I: Ipc, T: IntegrationTest> CongAlg<I> for TestBaseConfig<T> {
    type Flow = TestBase<I, T>;

    fn name() -> &'static str {
        "integration-test"
    }

    fn datapath_programs(&self) -> HashMap<&'static str, String> {
        T::datapath_programs()
    }

    fn new_flow(&self, control: Datapath<I>, _info: DatapathInfo) -> Self::Flow {
        let mut tb = TestBase {
            control_channel: control,
            sc: Default::default(),
            test_start: std::time::Instant::now(),
            sender: self.0.clone(),
            t: T::new(),
        };

        tb.sc = tb.t.install_test(&mut tb.control_channel);
        tb
    }
}

pub struct TestBase<I: Ipc, T: IntegrationTest> {
    pub control_channel: Datapath<I>,
    pub sc: Option<Scope>,
    pub test_start: std::time::Instant,
    pub sender: mpsc::Sender<Result<(), ()>>,
    t: T,
}

impl<I: Ipc, T: IntegrationTest> Flow for TestBase<I, T> {
    fn on_report(&mut self, sock_id: u32, m: Report) {
        let sc = self.sc.as_ref().unwrap();
        let done = self.t.check_test(sc, self.test_start, sock_id, &m);
        if done {
            self.sender.send(Ok(())).unwrap();
        }
    }
}

use portus::ipc::chan::Socket;
use portus::ipc::{BackendBuilder, Blocking};
use std::thread;

// Spawn userspace ccp
fn start_ccp<T: IntegrationTest + 'static + Send>(
    sk: Socket<Blocking>,
    tx: mpsc::Sender<Result<(), ()>>,
) -> portus::CCPHandle {
    portus::RunBuilder::new(BackendBuilder { sock: sk })
        .default_alg(TestBaseConfig(tx, PhantomData::<T>))
        .spawn_thread()
        .run()
        .unwrap()
}

// Runs a specific intergration test
pub fn run_test<T: IntegrationTest + 'static + Send>(num_flows: usize) {
    let (tx, rx) = std::sync::mpsc::channel();

    // Channel for IPC
    let (s1, r1) = crossbeam::channel::unbounded();
    let (s2, r2) = crossbeam::channel::unbounded();

    // spawn libccp
    let (dp_handle, conn_handles) = mock_datapath::start(num_flows, s2, r1);

    let sk = Socket::<Blocking>::new(s1, r2);
    let ccp_handle = start_ccp::<T>(sk, tx);

    // wait for program to finish
    let wait_for_done = thread::spawn(move || {
        rx.recv_timeout(std::time::Duration::from_secs(20))
            .unwrap()
            .unwrap();
        ccp_handle.kill(); // causes backend to stop iterating
        ccp_handle.wait().unwrap();

        for h in conn_handles {
            h.cancel();
            h.wait().unwrap();
        }

        dp_handle.cancel();
        dp_handle.wait().unwrap_or_else(|_| ());
    });

    wait_for_done.join().unwrap();
}
