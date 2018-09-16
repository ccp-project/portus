use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;

use super::{DONE, TestBase, IntegrationTestConfig};

pub struct TestUpdateFields<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestUpdateFields<T> {
    fn install_test(&mut self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.set_program(String::from("TestUpdateFields"), None).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let cwnd = m.get_field("Report.cwnd", sc).expect(
            "expected datapath cwnd field in returned measurement"
        ) as u32;
        let rate = m.get_field("Report.rate", sc).expect(
            "expected rate field in returned measurement"
         ) as u32;
        assert!(cwnd == 42,
                "Report in install_update contains wrong answer for cwnd, expected: {}, got: {}",
                42, cwnd);
        assert!(rate == 10,
                "Report in install_update contains wrong answer for rate, expected: {}, got: {}",
                42, rate);
        self.0.logger.as_ref().map(|log| {
            info!(log, "Passed update fields test.")
        });
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestUpdateFields<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

		fn init_programs(_cfg: Config<T, Self>) -> Vec<(String, String)> {
				vec![(String::from("TestUpdateFields"), String::from("
          	(def (Report.acked 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked Ack.bytes_acked)
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (== Cwnd 42)
                (report)
            )
            (when (> Micros 10000000) 
                (report)
            )
					")),
				]
    }


    fn create(control: Datapath<T>, cfg: Config<T, TestUpdateFields<T>>, _info: DatapathInfo) -> Self {
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
        {
            let sc = s.0.sc.as_ref().unwrap();
            s.0.control_channel.update_field(sc, &[("Cwnd", 42u32), ("Rate", 10u32)]).unwrap();
        }
        s
    }

    fn on_report(&mut self, _sock_id: u32, m: Report) {
        self.check_test(&m);
        self.0.sender.send(String::from(DONE)).unwrap();
    }
}
