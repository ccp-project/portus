extern crate fnv;
extern crate portus;
#[macro_use]
extern crate slog;
extern crate failure;
extern crate libccp;
extern crate minion;
extern crate slog_term;
extern crate time;

use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use std::collections::HashMap;

mod libccp_integration;
use crate::libccp_integration::IntegrationTest;

pub struct TestUpdateFields;

impl IntegrationTest for TestUpdateFields {
    fn new() -> Self {
        TestUpdateFields {}
    }

    fn datapath_programs() -> HashMap<&'static str, String> {
        let mut h = HashMap::default();
        h.insert(
            "TestUpdateFields",
            "
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
            )"
            .to_owned(),
        );
        h
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        let sc = dp.set_program("TestUpdateFields", None).ok()?;
        dp.update_field(&sc, &[("Cwnd", 42u32), ("Rate", 10u32)])
            .unwrap();
        Some(sc)
    }

    fn check_test(
        &mut self,
        sc: &Scope,
        _log: &slog::Logger,
        _t: std::time::Instant,
        _sock_id: u32,
        m: &Report,
    ) -> bool {
        let cwnd =
            m.get_field("Report.cwnd", sc)
                .expect("expected datapath cwnd field in returned measurement") as u32;
        let rate = m
            .get_field("Report.rate", sc)
            .expect("expected rate field in returned measurement") as u32;
        assert!(
            cwnd == 42,
            "Report in install_update contains wrong answer for cwnd, expected: {}, got: {}",
            42,
            cwnd
        );
        assert!(
            rate == 10,
            "Report in install_update contains wrong answer for rate, expected: {}, got: {}",
            42,
            rate
        );

        true
    }
}

#[test]
fn update() {
    let log = libccp_integration::logger();
    info!(log, "starting update test");
    libccp_integration::run_test::<TestUpdateFields>(log, 1);
}
