use portus::{Config, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;
use slog;

use super::{TestBase, IntegrationTest};

pub struct TestVolatileVars;

impl<T: Ipc> IntegrationTest<T> for TestVolatileVars {
    fn new() -> Self {
        TestVolatileVars{}
    }

    fn init_programs(_cfg: Config<T, TestBase<T, Self>>) -> Vec<(String, String)> {
        vec![(String::from("TestVolatileVars"), String::from("
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
            )")),
        ]
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        dp.set_program(String::from("TestVolatileVars"), None).ok()
    }

    fn check_test(&mut self, sc: &Scope, log: &slog::Logger, _t: SystemTime, _sock_id: u32, m: &Report) -> bool {
        let foo = m.get_field("Report.foo", sc).expect("get Report.foo");
        let bar = m.get_field("Report.bar", sc).expect("get Report.bar");

        assert_eq!(foo, 10);
        if bar == 10 {
            false
        } else {
            assert_eq!(bar, 20);
            info!(log, "Passed volatility test.");
            true
        }
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
        run_test::<super::TestVolatileVars>(log, 1);
    }
}
