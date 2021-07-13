use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use std::collections::HashMap;
use tracing::info;

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
    info!("starting preset test");
    libccp_integration::run_test::<TestPresetVars>(1);
}
