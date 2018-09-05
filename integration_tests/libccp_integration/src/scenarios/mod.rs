use portus::Datapath;
use portus::ipc::Ipc;
use portus::lang::Scope;
use std::time::SystemTime;
use std::sync::mpsc;
use slog;

pub const ACKED_PRIMITIVE: u32 = 5; // libccp uses this same value for acked_bytes
pub const DONE: &str = "Done";

#[derive(Clone)]
pub struct IntegrationTestConfig {
    pub sender: mpsc::Sender<String>
}

pub struct TestBase<T: Ipc> {
    pub control_channel: Datapath<T>,
    pub logger: Option<slog::Logger>,
    pub sc: Option<Scope>,
    pub test_start: SystemTime,
    pub sender: mpsc::Sender<String>,
}

mod basic;
mod preset;
mod timing;
mod update;
mod volatile;

pub use self::basic::TestBasicSerialize;
pub use self::preset::TestPresetVars;
pub use self::timing::TestTiming;
pub use self::update::TestUpdateFields;
pub use self::volatile::TestVolatileVars;
