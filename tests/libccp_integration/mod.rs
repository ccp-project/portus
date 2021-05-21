use std::marker::PhantomData;
use std::sync::mpsc;

use portus::ipc::Ipc;
use portus::lang::Scope;
use portus::{CongAlg, Datapath, DatapathInfo, DatapathTrait, Flow, Report};
use slog::{o, Drain};
use std::collections::HashMap;

pub const ACKED_PRIMITIVE: u32 = 5; // libccp uses this same value for acked_bytes

mod mock_datapath;

pub trait IntegrationTest: Sized {
    fn new() -> Self;
    fn datapath_programs() -> HashMap<&'static str, String>;
    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope>;
    fn check_test(
        &mut self,
        sc: &Scope,
        log: &slog::Logger,
        t: std::time::Instant,
        sock_id: u32,
        m: &Report,
    ) -> bool;
}

pub struct TestBaseConfig<T: IntegrationTest>(
    mpsc::Sender<Result<(), ()>>,
    Option<slog::Logger>,
    PhantomData<T>,
);

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
            logger: self.1.clone(),
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
    pub logger: Option<slog::Logger>,
    pub sc: Option<Scope>,
    pub test_start: std::time::Instant,
    pub sender: mpsc::Sender<Result<(), ()>>,
    t: T,
}

impl<I: Ipc, T: IntegrationTest> Flow for TestBase<I, T> {
    fn on_report(&mut self, sock_id: u32, m: Report) {
        let sc = self.sc.as_ref().unwrap();
        let l = self.logger.as_ref().unwrap();
        let done = self.t.check_test(sc, l, self.test_start, sock_id, &m);
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
    log: slog::Logger,
    tx: mpsc::Sender<Result<(), ()>>,
) -> portus::CCPHandle {
    portus::RunBuilder::new(
        BackendBuilder { sock: sk },
        portus::Config {
            logger: Some(log.clone()),
        },
    )
    .default_alg(TestBaseConfig(tx, Some(log.clone()), PhantomData::<T>))
    .spawn_thread()
    .run()
    .unwrap()
}

// Runs a specific intergration test
pub fn run_test<T: IntegrationTest + 'static + Send>(log: slog::Logger, num_flows: usize) {
    let (tx, rx) = std::sync::mpsc::channel();

    // Channel for IPC
    let (s1, r1) = crossbeam::channel::unbounded();
    let (s2, r2) = crossbeam::channel::unbounded();

    // spawn libccp
    let dp_log = log.clone();
    let (dp_handle, conn_handles) = mock_datapath::start(dp_log, num_flows, s2, r1);

    let sk = Socket::<Blocking>::new(s1, r2);
    let ccp_handle = start_ccp::<T>(sk, log.clone(), tx);

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

pub fn logger() -> slog::Logger {
    let decorator = slog_term::PlainSyncDecorator::new(slog_term::TestStdoutWriter);
    let human_drain = slog_term::FullFormat::new(decorator)
        .build()
        .filter_level(slog::Level::Debug)
        .fuse();
    slog::Logger::root(human_drain, o!())
}
