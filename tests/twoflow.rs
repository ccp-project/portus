#[cfg_attr(test, macro_use)]
extern crate slog;

use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use std::collections::HashMap;
use std::time::Instant;

mod libccp_integration;
use crate::libccp_integration::IntegrationTest;

pub struct TestTwoFlows;

static mut FLOW1_RECVD: u32 = 0;
static mut FLOW2_RECVD: u32 = 0;

impl IntegrationTest for TestTwoFlows {
    fn new() -> Self {
        TestTwoFlows {}
    }

    fn datapath_programs() -> HashMap<&'static str, String> {
        // compile and return any programs to be installed in the datapath
        let mut h = HashMap::default();
        h.insert(
            "TestTwoFlows",
            "(def (Control.number 0) (Report.value 0))
            (when (> Control.number 0)
                (:= Report.value Control.number)
                (:= Control.numer 0)
                (report)
            )"
            .to_owned(),
        );
        h
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        let sc = dp.set_program("TestTwoFlows", None).ok()?;
        let flow_num = dp.get_sock_id();
        dp.update_field(&sc, &[("Control.number", flow_num * 10)])
            .unwrap();
        Some(sc)
    }

    fn check_test(
        &mut self,
        sc: &Scope,
        _log: &slog::Logger,
        _t: Instant,
        sock_id: u32,
        m: &Report,
    ) -> bool {
        unsafe {
            let num = m
                .get_field("Report.value", sc)
                .expect("expected datapath value field in returned measurement")
                as u32;

            match sock_id {
                1 => FLOW1_RECVD = num,
                2 => FLOW2_RECVD = num,
                _ => unreachable!(),
            };

            match (FLOW1_RECVD, FLOW2_RECVD) {
                (10, 0) | (0, 20) => false,
                (0, _) | (_, 0) => false,
                (10, 20) => true,
                _ => {
                    assert_eq!(FLOW1_RECVD, 10);
                    assert_eq!(FLOW2_RECVD, 20);
                    unreachable!();
                }
            }
        }
    }
}

#[test]
fn twoflow() {
    let log = libccp_integration::logger();
    info!(log, "starting twoflow test");
    libccp_integration::run_test::<TestTwoFlows>(log, 2);
}
