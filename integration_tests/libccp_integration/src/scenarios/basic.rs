use portus::{Config, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;
use slog;

use super::{ACKED_PRIMITIVE, TestBase, IntegrationTest};

pub struct TestBasicSerialize;

impl<T: Ipc> IntegrationTest<T> for TestBasicSerialize {
    fn new() -> Self {
        TestBasicSerialize{}
    }

    fn init_programs(_cfg: Config<T, TestBase<T, Self>>) -> Vec<(String, String)> {
        // compile and return any programs to be installed in the datapath
        vec![(String::from("TestBasicSerialize"), String::from("
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
            )")),
        ]
    }


    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        dp.set_program(String::from("TestBasicSerialize"), None).ok()
    }

    fn check_test(&mut self, sc: &Scope, log: &slog::Logger, _t: SystemTime, m: &Report) -> bool {
        let acked = m.get_field("Report.acked", sc).expect(
            "expected acked field in returned measurement"
         ) as u32;
        let answer = 20 * ACKED_PRIMITIVE;
        assert!( acked == answer, 
                "Got wrong answer from basic test, expected: {}, got: {}", answer, acked);
        info!(log, "Passed basic serialization test.");
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
        run_test::<super::TestBasicSerialize>(log);
    }
}
