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

#[derive(Clone)]
pub struct IntegrationTestConfig {
    pub sender: mpsc::Sender<String>,
}

pub struct IntegrationTest<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Option<Scope>,
    current_test_start: SystemTime,
    current_test: u32,
    sender: mpsc::Sender<String>,
}

pub struct IntegrationTestMeasurements {
    pub acked: u32,
    pub cwnd: u32,
    pub rate: u32,
}

/// IntegrationTest contains tests, followed by checker functions
/// on_report and create loop through the tests and checker functions
impl<T: Ipc> IntegrationTest<T> {
    // get_fields used by all the tests
    fn get_fields(&mut self, m: &Report) -> IntegrationTestMeasurements {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let ack = m.get_field("Report.acked", sc).expect(
            "expected acked field in returned measurement"
         ) as u32;

        let cwnd = m.get_field("Report.cwnd", sc).expect(
            "expected datapath cwnd field in returned measurement"
        ) as u32;

        let rate = m.get_field("Report.rate", sc).expect(
            "expected datapath rate field in returned measurement"
        ) as u32;

        IntegrationTestMeasurements{
            acked: ack,
            cwnd: cwnd,
            rate: rate,
        }
    }

    // basic program: checks that report happens and contains the correct answer
    fn install_basic_test(&self) -> Option<Scope> {
        self.control_channel.install(
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
            ",
        ).ok()
    }

    // checks that the report from the baxic test contains the right answer
    fn check_basic_test(&mut self, m: &Report) -> bool {
        let ms = self.get_fields(&m);
        let answer = 20 * ACKED_PRIMITIVE;
        assert!( ms.acked == answer, 
                "Got wrong answer from basic test, expected: {}, got: {}", answer, ms.acked);
        self.current_test += 1;
        self.logger.as_ref().map(|log| {
            info!(log, "Passed basic serialization test.")
        });
        true
    }

    // timing test: checks timing of events, that a report comes back at roughly the correct time
    fn install_timing_test(&self) -> Option<Scope> {
        self.control_channel.install(
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
            ",
        ).ok()
    }

    // checks that the report comes at the right time
    fn check_timing_test(&mut self, m: &Report) -> bool {
        let ms = self.get_fields(&m);

        // check that it has roughly been 3 seconds
        let time_elapsed = self.current_test_start.elapsed().unwrap();
        assert!((time_elapsed >= Duration::from_secs(3) &&
                time_elapsed < Duration::from_secs(4)), 
                "Report in timing test received at not correct time, got: {}, expected 3 seconds", time_elapsed.subsec_nanos());

        // sanity check: acked primitive should be constant
        assert!( ms.acked == ACKED_PRIMITIVE, 
                "Got wrong answer from basic test, expected: {}, got: {}", ms.acked, ACKED_PRIMITIVE);
        self.current_test += 1;
        self.logger.as_ref().map(|log| {
            info!(log, "Passed timing test.")
        });
        true
    }

    // installs an update to the cwnd and rate registers to check updates happen
    fn install_update_test(&self) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        self.control_channel.install(
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
            ",
        ).ok()
    }

    fn check_install_update(&mut self, m: &Report) -> bool {
        // in returned measurement, check that Cwnd and Rate are set to new values
        let ms = self.get_fields(&m);
        assert!(ms.cwnd == 42,
                "Report in install_update contains wrong answer for cwnd, expected: {}, got: {}",
                42, ms.cwnd);
        assert!(ms.rate == 10,
                "Report in install_update contains wrong answer for rate, expected: {}, got: {}",
                42, ms.cwnd);
        self.current_test += 1;
        self.logger.as_ref().map(|log| {
            info!(log, "Passed update fields test.")
        });
        true
    }

    fn install_volatile_test(&self) -> Option<Scope> {
        self.control_channel.install(
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
            ",
        ).ok()
    }

    fn check_volatile_test(&mut self, m: &Report) -> bool {
        let sc = self.sc.as_ref().expect("scope should be initialized");
        let foo = m.get_field("Report.foo", sc).expect("get Report.foo");
        let bar = m.get_field("Report.bar", sc).expect("get Report.bar");

        assert_eq!(foo, 10);
        if bar == 10 {
            false
        } else {
            assert_eq!(bar, 20);
            self.logger.as_ref().map(|log| {
                info!(log, "Passed volatility test.")
            });
            true
        }
    }
}

impl<T: Ipc> CongAlg<T> for IntegrationTest<T> {
    type Config = IntegrationTestConfig;
	
    fn name() -> String {
        String::from("integration-test")
    }

    fn create(control: Datapath<T>, cfg: Config<T, IntegrationTest<T>>, _info: DatapathInfo) -> Self {  
        let mut s = Self {
            control_channel: control,
            sc: None,
            logger: cfg.logger,
            current_test_start: SystemTime::now(),
            current_test: 0,
            sender: cfg.config.sender.clone(),

        };

        s.logger.as_ref().map(|log| {
            debug!(log, "starting integration test flow");
        });
        // install first test 
        s.current_test_start = SystemTime::now();
        s.sc = s.install_basic_test(); 
        s
    }

    fn on_report(&mut self, _sock_id: u32, m: Report) {
        if self.current_test == 0 {
            self.check_basic_test(&m); 
            self.current_test_start = SystemTime::now();
            self.sc = self.install_timing_test();
        } else if self.current_test == 1 {
            self.check_timing_test(&m);
            self.sc = self.install_update_test();
            let sc = self.sc.as_ref().unwrap();
            // set Cwnd through an update_field message
            self.control_channel.update_field(sc, &[("Cwnd", 42u32), ("Rate", 10u32)]).unwrap();
        } else if self.current_test == 2 {
            self.check_install_update(&m);
            self.sc = self.install_volatile_test();
        } else if self.current_test == 3 {
            let done = self.check_volatile_test(&m);
            if done {
                self.current_test += 1;
                self.current_test_start = SystemTime::now();
            }
        } else if self.current_test == 4 {
            // send on the channel to close the integration test program
            self.logger.as_ref().map(|log| {
                info!(log, "Passed all integration tests!")
            });
            self.sender.send(String::from("Done!")).unwrap();
        }
    }
}
