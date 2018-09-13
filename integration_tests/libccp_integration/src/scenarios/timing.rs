use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::{Duration, SystemTime};

use super::{ACKED_PRIMITIVE, DONE, TestBase, IntegrationTestConfig};

pub struct TestTiming<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestTiming<T> {
    fn install_test(&mut self) -> Option<Scope> {
        self.0.control_channel.set_program(String::from("TestTiming"), None).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let acked = m.get_field("Report.acked", sc).expect(
            "expected acked field in returned measurement"
         ) as u32;
        // check that it has roughly been 3 seconds
        let time_elapsed = self.0.test_start.elapsed().unwrap();
        assert!((time_elapsed >= Duration::from_secs(3) &&
                time_elapsed < Duration::from_secs(4)), 
                "Report in timing test received at not correct time, got: {}, expected 3 seconds", time_elapsed.subsec_nanos());

        // sanity check: acked primitive should be constant
        assert!( acked == ACKED_PRIMITIVE, 
                "Got wrong answer from basic test, expected: {}, got: {}", acked, ACKED_PRIMITIVE);
        self.0.logger.as_ref().map(|log| {
            info!(log, "Passed timing test.")
        });
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestTiming<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

		fn init_programs() -> Vec<(String, String)> {
				vec![(String::from("TestTiming"), String::from("
						(def (Report.acked 0) (Control.state 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked Ack.bytes_acked)
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (&& (> Micros 3000000) (== Control.state 0))
                (:= Control.state 1)
                (report)
            )
					")),
				]
    }

    fn create(control: Datapath<T>, cfg: Config<T, TestTiming<T>>, _info: DatapathInfo) -> Self {
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
