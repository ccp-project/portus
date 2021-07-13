use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use std::collections::HashMap;
use std::time::Instant;
use tracing::info;

mod libccp_integration;
use crate::libccp_integration::{IntegrationTest, ACKED_PRIMITIVE};

pub struct TestBasicSerialize;

impl IntegrationTest for TestBasicSerialize {
    fn new() -> Self {
        TestBasicSerialize {}
    }

    fn datapath_programs() -> HashMap<&'static str, String> {
        // compile and return any programs to be installed in the datapath
        let mut h = HashMap::default();
        h.insert(
            "TestBasicSerialize",
            "
            (def (Report.acked 0) (Control.num_invoked 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked (+ Report.acked Ack.bytes_acked))
                (:= Control.num_invoked (+ Control.num_invoked 1))
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (== Control.num_invoked 20)
                (report)
            )"
            .to_owned(),
        );
        h
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        dp.set_program("TestBasicSerialize", None).ok()
    }

    fn check_test(&mut self, sc: &Scope, _t: Instant, _sock_id: u32, m: &Report) -> bool {
        let acked = m
            .get_field("Report.acked", sc)
            .expect("expected acked field in returned measurement") as u32;
        let answer = 20 * ACKED_PRIMITIVE;
        assert!(
            acked == answer,
            "Got wrong answer from basic test, expected: {}, got: {}",
            answer,
            acked
        );

        true
    }
}

#[test]
fn basic() {
    info!("starting basic test");
    libccp_integration::run_test::<TestBasicSerialize>(1);
}
