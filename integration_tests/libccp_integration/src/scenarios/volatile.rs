use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;

use super::{DONE, TestBase, IntegrationTestConfig};

pub struct TestVolatileVars<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestVolatileVars<T> {
    fn install_test(&mut self) -> Option<Scope> {
        self.0.control_channel.set_program(String::from("TestVolatileVars"), None).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let foo = m.get_field("Report.foo", sc).expect("get Report.foo");
        let bar = m.get_field("Report.bar", sc).expect("get Report.bar");

        assert_eq!(foo, 10);
        if bar == 10 {
            false
        } else {
            assert_eq!(bar, 20);
            self.0.logger.as_ref().map(|log| {
                info!(log, "Passed volatility test.")
            });
            true
        }
    }
}

impl<T: Ipc> CongAlg<T> for TestVolatileVars<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

		fn init_programs() -> Vec<(String, String)> {
				vec![(String::from("TestVolatileVars"), String::from("
            (def
                (Report
                    (volatile foo 0)
                    (bar 0))
            )
            (when true
                (:= Report.foo (+ Report.foo 1))
                (:= Report.bar (+ Report.bar 1))
                (fallthrough)
            )
            (when (== Report.foo 10)
                (report)
            )
					")),
				]
    }

    fn create(control: Datapath<T>, cfg: Config<T, TestVolatileVars<T>>, _info: DatapathInfo) -> Self {
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
        let done = self.check_test(&m);
        if done {
            self.0.sender.send(String::from(DONE)).unwrap();
        }
    }
}
