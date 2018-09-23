use portus::{Config, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::{Duration, SystemTime};
use slog;

use super::{ACKED_PRIMITIVE, TestBase, IntegrationTest};

pub struct TestTiming;

impl<T: Ipc> IntegrationTest<T> for TestTiming {
    fn new() -> Self {
        TestTiming{}
    }

    fn init_programs(_cfg: Config<T, TestBase<T, Self>>) -> Vec<(String, String)> {
        vec![(String::from("TestTiming"), String::from("
            (def (Report.acked 0) (Control.state 0) (Report.cwnd 0) (Report.rate 0))
            (when true
                (:= Report.acked Ack.bytes_acked)
                (:= Report.cwnd Cwnd)
                (:= Report.rate Rate)
                (fallthrough)
            )
            (when (&& (> Micros 3000000) (== Control.state 0))
                (:= Control.state 1)
                (report)
            )")),
		]
    }

    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        dp.set_program(String::from("TestTiming"), None).ok()
    }

    fn check_test(&mut self, sc: &Scope, log: &slog::Logger, t: SystemTime, _sock_id: u32, m: &Report) -> bool {
        let acked = m.get_field("Report.acked", sc).expect(
            "expected acked field in returned measurement"
         ) as u32;
        // check that it has roughly been 3 seconds
        let time_elapsed = t.elapsed().unwrap();
        assert!(
            (time_elapsed >= Duration::from_secs(3) && time_elapsed < Duration::from_secs(4)), 
            "Report in timing test received at not correct time, got: {}, expected 3 seconds", 
            time_elapsed.subsec_nanos(),
        );

        // sanity check: acked primitive should be constant
        assert!(
            acked == ACKED_PRIMITIVE, 
            "Got wrong answer from basic test, expected: {}, got: {}", 
            acked, 
            ACKED_PRIMITIVE,
        );
        info!(log, "Passed timing test.");
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
        run_test::<super::TestTiming>(log, 1);
    }
}
