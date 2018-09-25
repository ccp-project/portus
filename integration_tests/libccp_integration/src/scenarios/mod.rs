use portus;
use portus::ipc::Ipc;
use portus::lang::Scope;
use portus::{CongAlg, Config, Datapath, DatapathTrait, DatapathInfo, Report};

use std::time::SystemTime;
use std::sync::mpsc;
use slog;

pub const ACKED_PRIMITIVE: u32 = 5; // libccp uses this same value for acked_bytes
pub const DONE: &str = "Done";

#[derive(Clone)]
pub struct IntegrationTestConfig {
    pub sender: mpsc::Sender<String>
}

pub trait IntegrationTest<T: Ipc>: Sized {
    fn new() -> Self;
    fn init_programs(cfg: Config<T, TestBase<T, Self>>) -> Vec<(String, String)>;
    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope>;
    fn check_test(&mut self, sc: &Scope, log: &slog::Logger, t: SystemTime, sock_id: u32, m: &Report) -> bool;
}

pub struct TestBase<I: Ipc, T: IntegrationTest<I>> {
    pub control_channel: Datapath<I>,
    pub logger: Option<slog::Logger>,
    pub sc: Option<Scope>,
    pub test_start: SystemTime,
    pub sender: mpsc::Sender<String>,
    t: T,
}

impl<I: Ipc, T: IntegrationTest<I>> CongAlg<I> for TestBase<I, T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

    fn init_programs(cfg: Config<I, Self>) -> Vec<(String, String)> {
        T::init_programs(cfg)
    }

    fn create(control: Datapath<I>, cfg: Config<I, Self>, _info: DatapathInfo) -> Self {
        let mut tb = TestBase {
            control_channel: control,
            sc: Default::default(),
            logger: cfg.logger,
            test_start: SystemTime::now(),
            sender: cfg.config.sender.clone(),
            t: T::new(),
        };

        tb.sc = tb.t.install_test(&mut tb.control_channel);
        tb
    }
    
    fn on_report(&mut self, sock_id: u32, m: Report) {
        let sc = self.sc.as_ref().unwrap();
        let l = self.logger.as_ref().unwrap();
        let done = self.t.check_test(sc, l, self.test_start, sock_id, &m);
        if done {
            self.sender.send(String::from(DONE)).unwrap();
        }
    }
}

mod basic;
mod preset;
mod timing;
mod twoflow;
mod update;
mod volatile;

use std;
use std::thread;
use portus::ipc::{BackendBuilder, Blocking};
//use portus::ipc::chan::Socket;
use portus::ipc::unix::Socket;

// Spawn userspace ccp
fn start_ccp<T>(sk: Socket<Blocking>, log: slog::Logger, tx: mpsc::Sender<String>) -> portus::CCPHandle
    where T: portus::CongAlg<
        Socket<Blocking>,
        Config=IntegrationTestConfig,
    > + 'static
{
    let cfg = IntegrationTestConfig{
        sender: tx, // used for the algorithm to send a signal whent the tests are over
    };
    
    let b = BackendBuilder{ sock: sk };
    portus::spawn::<_, T>(
		b,
		portus::Config {
			logger: Some(log),
			config: cfg,
		}
	)
}

// Runs a specific intergration test
pub fn run_test<T: IntegrationTest<Socket<Blocking>> + 'static>(log: slog::Logger, num_flows: usize) {
    let (tx, rx) = mpsc::channel();

    // Channel for IPC
    //let (s1, r1) = mpsc::channel();
    //let (s2, r2) = mpsc::channel();
    // make UnixDatagram receiver
    std::fs::remove_file("/tmp/ccp/0/out").unwrap_or_else(|_| ());
    std::fs::create_dir_all("/tmp/ccp/0").unwrap();
    let recv_sk = std::os::unix::net::UnixDatagram::bind("/tmp/ccp/0/out").expect("make unix dp listener");
    recv_sk.set_read_timeout(Some(std::time::Duration::from_millis(1000))).unwrap();

    // spawn libccp
    let (mock_dp_ready_tx, mock_dp_ready_rx) = mpsc::channel();
    let (mock_dp_done_tx, mock_dp_done_rx) = mpsc::channel();
    let dp_log = log.clone();
    thread::spawn(move || {
        ::mock_datapath::start(mock_dp_done_rx, mock_dp_ready_tx, recv_sk, num_flows, dp_log);
    });

    use scenarios::TestBase;
    // wait for mock datapath to spawn
    mock_dp_ready_rx.recv().unwrap();
    //let sk = Socket::new(s2, r1).expect("ipc initialization");
    let sk = Socket::<Blocking>::new("in", "out").unwrap();
    let ccp_handle = start_ccp::<TestBase<Socket<Blocking>, T>>(sk, log.clone(), tx);

    // wait for program to finish
    let wait_for_done = thread::spawn(move ||{
        let msg = rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap();
        assert!(msg == DONE, "Received wrong message on channel");
        ccp_handle.kill(); // causes backend to stop iterating
        mock_dp_done_tx.send(()).unwrap();
        ccp_handle.wait().unwrap();
    });
    
    wait_for_done.join().unwrap();
}

pub fn log_commits(log: slog::Logger) {
    use std::str::from_utf8;
    use std::process::Command;
    let portus_commit = Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .expect("Failed to get portus commit hash")
        .stdout;
    let portus_branch = Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .expect("Failed to get portus branch name")
        .stdout;
    info!(log, "portus commit";
        "commit hash" => from_utf8(&portus_commit).unwrap().trim_right(),
        "branch" => from_utf8(&portus_branch).unwrap().trim_right(),
    );

    let libccp_commit = Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir("./libccp")
        .output()
        .expect("Failed to get libccp commit hash")
        .stdout;
    let libccp_branch = Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .current_dir("./libccp")
        .output()
        .expect("Failed to get libccp branch name")
        .stdout;
    info!(log, "libccp commit";
        "commit hash" => from_utf8(&libccp_commit).unwrap().trim_right(),
        "branch" => from_utf8(&libccp_branch).unwrap().trim_right(),
    );
}
