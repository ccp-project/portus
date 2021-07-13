use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::info;

mod libccp_integration;
use crate::libccp_integration::{IntegrationTest, ACKED_PRIMITIVE};

pub struct TestTiming;

impl IntegrationTest for TestTiming {
    fn new() -> Self {
        TestTiming {}
    }

    fn datapath_programs() -> HashMap<&'static str, String> {
        let mut h = HashMap::default();
        h.insert(
            "TestTiming",
            "
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
            )"
            .to_owned(),
        );
        h
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        dp.set_program("TestTiming", None).ok()
    }

    fn check_test(&mut self, sc: &Scope, t: Instant, _sock_id: u32, m: &Report) -> bool {
        let acked = m
            .get_field("Report.acked", sc)
            .expect("expected acked field in returned measurement") as u32;
        // check that it has roughly been 3 seconds
        let time_elapsed = t.elapsed();
        assert!(
            (time_elapsed >= Duration::from_secs(3) && time_elapsed < Duration::from_secs(4)),
            "Report in timing test received at not correct time, got: {}, expected 3 seconds",
            time_elapsed.as_secs(),
        );

        // sanity check: acked primitive should be constant
        assert!(
            acked == ACKED_PRIMITIVE,
            "Got wrong answer from basic test, expected: {}, got: {}",
            acked,
            ACKED_PRIMITIVE,
        );

        true
    }
}

#[test]
fn timing() {
    info!("starting timing test");
    libccp_integration::run_test::<TestTiming>(1);
}
