use std::time::SystemTime;

use fnv::FnvHashMap as HashMap;
use portus::lang::Scope;
use portus::{DatapathTrait, Report};
use slog;

use super::IntegrationTest;

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
        log: &slog::Logger,
        _t: SystemTime,
        _sock_id: u32,
        m: &Report,
    ) -> bool {
        let foo = m
            .get_field("Report.testFoo", sc)
            .expect("get Report.testFoo");

        assert_eq!(foo, 52, "Foo should be installed automaticaly as 52.");
        info!(log, "Passed preset vars test");
        true
    }
}

#[cfg(test)]
mod test {
    use scenarios::{log_commits, run_test};
    use slog;
    use slog::Drain;
    use slog_term;

    #[test]
    fn test() {
        let decorator = slog_term::PlainSyncDecorator::new(slog_term::TestStdoutWriter);
        let human_drain = slog_term::FullFormat::new(decorator)
            .build()
            .filter_level(slog::Level::Debug)
            .fuse();
        let log = slog::Logger::root(human_drain, o!());
        log_commits(log.clone());
        run_test::<super::TestPresetVars>(log, 1);
    }
}
