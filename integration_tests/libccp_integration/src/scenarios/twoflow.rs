use portus::{Config, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;
use slog;

use super::{TestBase, IntegrationTest};

pub struct TestTwoFlows;

static mut FLOW1_RECVD: u32 = 0;
static mut FLOW2_RECVD: u32 = 0;

impl<T: Ipc> IntegrationTest<T> for TestTwoFlows {
    fn new() -> Self {
        TestTwoFlows{}
    }

    fn init_programs(_cfg: Config<T, TestBase<T, Self>>) -> Vec<(String, String)> {
        // compile and return any programs to be installed in the datapath
        vec![(String::from("TestTwoFlows"), String::from("
            (def (Control.number 0) (Report.value 0))
            (when (> Control.number 0)
                (:= Report.value Control.number)
                (:= Control.numer 0)
                (report)
            )")),
        ]
    }
 
    fn install_test<D: DatapathTrait>(&self, dp: &mut D) -> Option<Scope> {
        let sc = dp.set_program(String::from("TestTwoFlows"), None).ok()?;
        let flow_num = dp.get_sock_id();
        dp.update_field(&sc, &[("Control.number", flow_num * 10)]).unwrap();
        println!("start flow {}", flow_num);
        Some(sc)
    }

    fn check_test(&mut self, sc: &Scope, _log: &slog::Logger, _t: SystemTime, sock_id: u32, m: &Report) -> bool { unsafe {
        let num = m.get_field("Report.value", sc).expect(
            "expected datapath value field in returned measurement"
        ) as u32;

        match sock_id {
            1 => FLOW1_RECVD = num,
            2 => FLOW2_RECVD = num,
            _ => unreachable!()
        };

        match (FLOW1_RECVD, FLOW2_RECVD) {
            (10,0) | (0,20) => false,
            (0,_)  | (_,0)  => false,
            (10,20)         => true,
            _               => {
                assert_eq!(FLOW1_RECVD, 10);
                assert_eq!(FLOW2_RECVD, 20);
                unreachable!();
            }
        }

    } }
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
        run_test::<super::TestTwoFlows>(log, 2);
    }
}
