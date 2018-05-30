//! This crate implements integration tests for libccp and portus.
//! Each integration test consists of the following;:
//! 1. A struct IntegrationTest<T: Ipc>, along with the Impl.
//!     This could be a tuple struct around TestBase, declared as:
//!     pub struct IntegrationTest<T: Ipc>(TestBase<T>)
//! 2. Any additional structs for use with struct IntegrationTest, 
//!     such as an IntegrationTestMeasurements struct
//! 3. impl<T: Ipc> CongAlg<T> for IntegrationTest<T>
//!     - This contains the onCreate() and onReport().
//!     on_create() might install a program implemented in the IntegrationTest,
//!     while on_report() might contain a checker function for the test.
//!     on_report MUST send "Done" on the channel to end the test properly.

extern crate clap;
extern crate time;
#[macro_use]
extern crate slog;
extern crate portus;

use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::{Duration, SystemTime};
use std::sync::mpsc;
const ACKED_PRIMITIVE: u32 = 5; // libccp uses this same value for acked_bytes
pub const DONE: &str = "Done";

#[derive(Clone)]
pub struct IntegrationTestConfig {
    pub sender: mpsc::Sender<String>
}

pub struct TestBase<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    test_start: SystemTime,
    sender: mpsc::Sender<String>,
}

pub struct TestBasicSerialize<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestBasicSerialize<T> {
    fn install_test(&self) -> Option<Scope> {
        println!("Installing test");
        self.0.control_channel.install(
            b" (def (Report.acked 0) (Control.num_invoked 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked (+Report.acked Ack.bytes_acked))
                (:= Control.num_invoked (+Control.num_invoked 1))
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (== Control.num_invoked 20)
                (report)
            )
            ", None
        ).ok()
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

    fn create(control: Datapath<T>, cfg: Config<T, TestBasicSerialize<T>>, _info: DatapathInfo) -> Self {
        println!("Calling create");
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

pub struct TestTiming<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestTiming<T> {
    fn install_test(&self) -> Option<Scope> {
        self.0.control_channel.install(
            b" (def (Report.acked 0) (Control.state 0) (Report.cwnd 0) (Report.rate 0))
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
            ", None
        ).ok()
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

pub struct TestUpdateFields<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestUpdateFields<T> {
    fn install_test(&self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.install(
            b" (def (Report.acked 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked Ack.bytes_acked)
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (== Cwnd 42)
                (report)
            )
            ", None
        ).ok()

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

pub struct TestVolatileVars<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestVolatileVars<T> {
    fn install_test(&self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.install(
            b"
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
            ", None
        ).ok()
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

pub struct TestPresetVars<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestPresetVars<T> {
    fn install_test(&self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.install(
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
            )
            ", Some(&[("foo", 52)][..])
        ).ok()
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

pub struct TestLongProgram<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestLongProgram<T> {
    fn install_test(&self) -> Option<Scope> {
        // emulate a long program, modeled loosely after the bbr state machine
        self.0.control_channel.install(
            b"
            (def
                (Report
                    (reportVar1 0)
                    (reportVar2 0)
                    (reportVar3 0)
                    (reportVar4 0)
                    (reportVar5 0)
                    (reportVar6 0)
                    (reportVar7 0)
                    (volatile loss 0)
                    (volatile minrtt +infinity)
                )
                (controlVar1 0)
            )
            (when true
                (:= Report.loss (+ Report.loss Ack.lost_pkts_sample))
                (:= Report.minrtt (min Report.minrtt Flow.rtt_sample_us))
                (fallthrough)
            )
            (when (&& (> Micros 3000) (== controlVar1 0))
                (:= controlVar1 1)
                (:= Report.reportVar1 (max Flow.rtt_sample_us 3000))
                (:= Report.reportVar5 (+ Report.reportVar5 Ack.bytes_acked))
                (fallthrough)
            )
            (when (&& (> Micros 6000) (== controlVar1 1))
                (:= controlVar1 2)
                (:= Report.reportVar2 (max Flow.rtt_sample_us 6000))
                (:= Report.reportVar6 (+ Report.reportVar5 Ack.bytes_acked))
                (fallthrough)
            )
            (when (&& (> Micros 9000) (== controlVar1 2))
                (:= controlVar1 3)
                (:= Report.reportVar3 (max Flow.rtt_sample_us 9000))
                (:= Report.reportVar7 (+ Report.reportVar6 Ack.bytes_acked))
                (fallthrough)
            )
            (when (&& (> Micros 12000) (== controlVar1 3))
                (:= Report.reportVar4 (max Flow.rtt_sample_us 12000))
                (report)
            )
            ", None
        ).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let var1 = m.get_field("Report.reportVar1", sc).expect("get Report.reportVar1");
        let var2 = m.get_field("Report.reportVar2", sc).expect("get Report.reportVar2");
        let var3 = m.get_field("Report.reportVar3", sc).expect("get Report.reportVar3");
        let var4 = m.get_field("Report.reportVar4", sc).expect("get Report.reportVar4");
        let var5 = m.get_field("Report.reportVar5", sc).expect("get Report.reportVar5");
        let var6 = m.get_field("Report.reportVar6", sc).expect("get Report.reportVar6");
        let var7 = m.get_field("Report.reportVar7", sc).expect("get Report.reportVar7");

        assert_eq!(var1, 3000, "Var1 should be installed as 3000.");
        assert_eq!(var2, 6000, "Var1 should be installed as 6000.");
        assert_eq!(var3, 9000, "Var1 should be installed as 9000.");
        assert_eq!(var4, 12000, "Var1 should be installed as 12000.");
        assert_eq!(var5, ACKED_PRIMITIVE as u64, "Var5 should be the acked value.");
        assert_eq!(var6, (ACKED_PRIMITIVE*2) as u64, "Var6 should be the acked value * 2.");
        assert_eq!(var7, (ACKED_PRIMITIVE*3) as u64, "Var7 should be the acked value * 3.");
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestLongProgram<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

    fn create(control: Datapath<T>, cfg: Config<T, TestLongProgram<T>>, _info: DatapathInfo) -> Self {
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

pub struct TestMultipleTrueConditions<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestMultipleTrueConditions<T> {
    fn install_test(&self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.install(
            b"
            (def
                (Report
                    (testVar1 0)
                    (testVar2 0)
                )
                (controlVar1 0)
            )
            (when true
                (:= Report.testVar1 10)
                (fallthrough)
            )
            (when (> Micros 0)
                (:= controlVar1 2)
                (fallthrough)
            )
            (when true
                (:= Report.testVar2 10)
                (report)
            )
            ", None
        ).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let var1 = m.get_field("Report.testVar1", sc).expect("get Report.testVar1");
        let var2 = m.get_field("Report.testVar2", sc).expect("get Report.testVar2");
        assert_eq!(var1, 10, "Var1 should automatically be set to 10.");
        assert_eq!(var2, 10, "Var2 should automatically be set to 10.");
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestMultipleTrueConditions<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

    fn create(control: Datapath<T>, cfg: Config<T, TestMultipleTrueConditions<T>>, _info: DatapathInfo) -> Self {
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
        println!("Installing test next");
        s.0.sc = s.install_test();
        println!("Installed test");
        s
    }

    fn on_report(&mut self, _sock_id: u32, m: Report) {
        let done = self.check_test(&m);
        if done {
            self.0.sender.send(String::from(DONE)).unwrap();
        }
    }
}

pub struct TestIfNotIf<T: Ipc>(TestBase<T>);

impl<T: Ipc> TestIfNotIf<T> {
    fn install_test(&self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.0.control_channel.install(
            b"
            (def
                (Report
                    (testVar1 0)
                )
            )
            (when true
                (:= Report.testVar1 10)
                (fallthrough)
            )
            (when (== Report.testVar1 10))
                (report)
            )
            ", None
        ).ok()
    }

    fn check_test(&mut self, m: &Report) -> bool {
        let sc = self.0.sc.as_ref().expect("scope should be initialized");
        let var1 = m.get_field("Report.testVar1", sc).expect("get Report.testVar1");
        assert_eq!(var1, 10, "Var1 should automatically be set to 10.");
        true
    }
}

impl<T: Ipc> CongAlg<T> for TestIfNotIf<T> {
    type Config = IntegrationTestConfig;
    fn name() -> String {
        String::from("integration-test")
    }

    fn create(control: Datapath<T>, cfg: Config<T, TestIfNotIf<T>>, _info: DatapathInfo) -> Self {
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

