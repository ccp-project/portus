extern crate fnv;
extern crate portus;
#[macro_use]
extern crate slog;
extern crate failure;
extern crate libccp;
extern crate minion;
extern crate slog_term;
extern crate time;

use std::collections::HashMap;
use portus::lang::Scope;
use portus::{DatapathTrait, Report};

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

        assert_eq!(foo, 10);
        if bar == 10 {
            false
        } else {
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
