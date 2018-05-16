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
