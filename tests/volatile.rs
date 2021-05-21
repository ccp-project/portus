use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use slog::info;
use std::collections::HashMap;

mod libccp_integration;
use crate::libccp_integration::IntegrationTest;

pub struct TestVolatileVars;

impl IntegrationTest for TestVolatileVars {
    fn new() -> Self {
        TestVolatileVars {}
    }

    fn datapath_programs() -> HashMap<&'static str, String> {
        let mut h = HashMap::default();
        h.insert(
            "TestVolatileVars",
            "
            (def
                (volatile foo1 0)
                (Report
                    (volatile foo 0)
                    (bar 0)
                    (volatile sum 0)
                )
                (bar1 0)
            )
            (when true
                (:= Report.foo (+ Report.foo 1))
                (:= Report.bar (+ Report.bar 1))
                (:= foo1 (+ foo1 1))
                (:= bar1 (+ bar1 1))
                (:= Report.sum (+ foo1 bar1))
                (fallthrough)
            )
            (when (== Report.foo 10)
                (report)
            )"
            .to_owned(),
        );
        h
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        dp.set_program("TestVolatileVars", None).ok()
    }

    fn check_test(
        &mut self,
        sc: &Scope,
        _log: &slog::Logger,
        _t: std::time::Instant,
        _sock_id: u32,
        m: &Report,
    ) -> bool {
        let foo = m.get_field("Report.foo", sc).expect("get Report.foo");
        let bar = m.get_field("Report.bar", sc).expect("get Report.bar");
        let sum = m.get_field("Report.sum", sc).expect("get Report.sum");

        assert_eq!(foo, 10);
        if bar == 10 {
            assert_eq!(sum, 20);
            false
        } else {
            assert_eq!(sum, 30);
            assert_eq!(bar, 20);
            true
        }
    }
}

#[test]
fn volatile() {
    let log = libccp_integration::logger();
    info!(log, "starting volatile test");
    libccp_integration::run_test::<TestVolatileVars>(log, 1);
}
