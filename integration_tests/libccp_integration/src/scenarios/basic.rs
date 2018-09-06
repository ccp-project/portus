use portus;
use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::{Bin, Scope};
use std::time::SystemTime;

use super::{ACKED_PRIMITIVE, DONE, TestBase, IntegrationTestConfig};

pub struct TestBasicSerialize<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestBasicSerialize<T> {
    fn install_test(&mut self) -> Option<Scope> {
        self.0.control_channel.set_program(String::from("TestBasicSerialize"), None).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let acked = m.get_field("Report.acked", sc).expect(
            "expected acked field in returned measurement"
         ) as u32;
        let answer = 20 * ACKED_PRIMITIVE;
        assert!( acked == answer, 
                "Got wrong answer from basic test, expected: {}, got: {}", answer, acked);
        self.0.logger.as_ref().map(|log| {
            info!(log, "Passed basic serialization test.")
        });
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestBasicSerialize<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

    fn init_programs() -> Option<Vec<(Bin, Scope, String)>> {
        // compile and return any programs to be installed in the datapath
        let prog = b" (def (Report.acked 0) (Control.num_invoked 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked (+ Report.acked Ack.bytes_acked))
                (:= Control.num_invoked (+ Control.num_invoked 1))
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (== Control.num_invoked 20)
                (report)
            )
            ";
        let (bin, sc) = portus::compile_program(prog, None).unwrap(); // better error handling?
        Some(vec![(bin, sc, String::from("TestBasicSerialize"))])
    } 

    fn create(control: Datapath<T>, cfg: Config<T, TestBasicSerialize<T>>, _info: DatapathInfo) -> Self {
        let mut s = Self {
            0: TestBase {
                control_channel: control,
                sc: Default::default(),
                logger: cfg.logger,
                test_start: SystemTime::now(),
                sender: cfg.config.sender.clone(),
            }
        };

        s.0.test_start = SystemTime::now();
        s.0.sc = s.install_test();
        s
    }

    fn on_report(&mut self, _sock_id: u32, m: Report) {
        self.check_test(&m);
        self.0.sender.send(String::from(DONE)).unwrap();
    }
}
