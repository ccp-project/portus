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

pub struct TestPresetVars;

impl IntegrationTest for TestPresetVars {
    fn new() -> Self {
        TestPresetVars {}
    }

    fn datapath_programs() -> HashMap<&'static str, String> {
        let mut h = HashMap::default();
        h.insert(
            "TestPresetVars",
            "
            (def
                (Report
                    (testFoo 0)
                )
                (foo 0)
            )
            (when true
                (:= Report.testFoo foo)
                (report)
            )"
            .to_owned(),
        );
        h
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        dp.set_program("TestPresetVars", Some(&[("foo", 52)][..]))
            .ok()
    }

    fn check_test(
        &mut self,
        sc: &Scope,
        _log: &slog::Logger,
        _t: std::time::Instant,
        _sock_id: u32,
        m: &Report,
    ) -> bool {
        let foo = m
            .get_field("Report.testFoo", sc)
            .expect("get Report.testFoo");

        assert_eq!(foo, 52, "Foo should be installed automaticaly as 52.");
        true
    }
}

#[test]
fn preset() {
    let log = libccp_integration::logger();
    info!(log, "starting preset test");
    libccp_integration::run_test::<TestPresetVars>(log, 1);
}
