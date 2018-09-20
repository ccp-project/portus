use portus::{Config, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;
use slog;

use super::{TestBase, IntegrationTest};

pub struct TestPresetVars;

impl<T: Ipc> IntegrationTest<T> for TestPresetVars {
    fn new() -> Self {
        TestPresetVars{}
    }

    fn init_programs(_cfg: Config<T, TestBase<T, Self>>) -> Vec<(String, String)> {
        vec![(String::from("TestPresetVars"), String::from("
            (def
                (Report
                    (testFoo 0)
                )
                (foo 0)
            )
            (when true
                (:= Report.testFoo foo)
                (report)
            )")),
        ]
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        // fold function that only reports when Cwnd is set to 42
        dp.set_program(String::from("TestPresetVars"), Some(&[("foo", 52)][..])).ok()
    }

    fn check_test(&mut self, sc: &Scope, log: &slog::Logger, _t: SystemTime, m: &Report) -> bool {
        let foo = m.get_field("Report.testFoo", sc).expect("get Report.testFoo");

        assert_eq!(foo, 52, "Foo should be installed automaticaly as 52.");
        info!(log, "Passed preset vars test");
        true
    }
}

#[cfg(test)]
mod test {
    use slog;
    use slog::Drain;
    use slog_term;
    use ::scenarios::{log_commits, run_test};

    #[test]
    fn test() {
        let decorator = slog_term::PlainSyncDecorator::new(slog_term::TestStdoutWriter);
        let human_drain = slog_term::FullFormat::new(decorator).build().filter_level(slog::Level::Debug).fuse();
        let log = slog::Logger::root(human_drain, o!());
        log_commits(log.clone());
        run_test::<super::TestPresetVars>(log);
    }
}
