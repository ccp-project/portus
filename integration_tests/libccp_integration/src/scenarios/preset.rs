use portus;
use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::{Bin, Scope};
use std::time::SystemTime;

use super::{DONE, TestBase, IntegrationTestConfig};

pub struct TestPresetVars<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestPresetVars<T> {
    fn install_test(&mut self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.set_program(String::from("TestPresetVars"), Some(&[("foo", 52)][..])).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let foo = m.get_field("Report.testFoo", sc).expect("get Report.testFoo");

        assert_eq!(foo, 52, "Foo should be installed automaticaly as 52.");
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestPresetVars<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

    fn init_programs() -> Option<Vec<(Bin, Scope, String)>> {
        let prog =
            b"
            (def
                (Report
                    (testFoo 0)
                )
                (foo 0)
            )
            (when true
                (:= Report.testFoo foo)
                (report)
            )";

        let (bin, sc) = portus::compile_program(prog, None).unwrap(); // better error handling?
        Some(vec![(bin, sc, String::from("TestPresetVars"))])
    }

    fn create(control: Datapath<T>, cfg: Config<T, TestPresetVars<T>>, _info: DatapathInfo) -> Self {
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
