//! Helper methods for making algorithm binaries.

/// Platform-dependent validator for ipc mechanisms.
#[cfg(all(target_os = "linux"))]
pub fn ipc_valid(v: String) -> std::result::Result<(), String> {
    match v.as_str() {
        "netlink" | "unix" | "char" => Ok(()),
        _ => Err(format!("ipc must be one of (netlink|unix|char): {:?}", v)),
    }
}

/// Platform-dependent validator for ipc mechanisms.
#[cfg(not(target_os = "linux"))]
pub fn ipc_valid(v: String) -> std::result::Result<(), String> {
    match v.as_str() {
        "unix" => Ok(()),
        _ => Err(format!("ipc must be one of (unix): {:?}", v)),
    }
}

/// Convenience macro for starting the portus runtime in the common
/// single-algorithm case. The 3-argument form will use blocking IPC sockets.
/// Arguments are:
/// 1. ipc, a &str specifying the IPC type
/// (either "unix", "netlink", or "char"): see [`ipc`](./ipc/index.html).
/// 2. alg, an instance of `impl CongAlg<T: Ipc>`.
/// 3. blk, optional argument, either [`Blocking`](./ipc/struct.Blocking.html) or
///    [`Nonblocking`](./ipc/struct.Nonblocking.html).
///
///
/// # Example
///
/// Using the example algorithm from above:
///
/// ```
/// use std::collections::HashMap;
/// use portus::{CongAlg, Flow, Datapath, DatapathInfo, DatapathTrait, Report};
/// use portus::ipc::Ipc;
/// use portus::lang::Scope;
/// use portus::lang::Bin;
///
/// #[derive(Clone, Default)]
/// struct MyCongestionControlAlgorithm(Scope);
///
/// impl<I: Ipc> CongAlg<I> for MyCongestionControlAlgorithm {
///     type Flow = Self;
///
///     fn name() -> &'static str {
///         "My congestion control algorithm"
///     }
///     fn datapath_programs(&self) -> HashMap<&'static str, String> {
///         let mut h = HashMap::default();
///         h.insert(
///             "MyProgram", "
///                 (def (Report
///                     (volatile minrtt +infinity)
///                 ))
///                 (when true
///                     (:= Report.minrtt (min Report.minrtt Flow.rtt_sample_us))
///                 )
///                 (when (> Micros 42000)
///                     (report)
///                     (reset)
///                 )
///             ".to_owned(),
///         );
///         h
///     }
///     fn new_flow(&self, mut control: Datapath<I>, info: DatapathInfo) -> Self::Flow {
///         let sc = control.set_program("MyProgram", None).unwrap();
///         MyCongestionControlAlgorithm(sc)
///     }
/// }
/// impl Flow for MyCongestionControlAlgorithm {
///     fn on_report(&mut self, sock_id: u32, m: Report) {
///         println!("minrtt: {:?}", m.get_field("Report.minrtt", &self.0).unwrap());
///     }
/// }
///
/// fn main() {
///     portus::start!("unix", None, MyCongestionControlAlgorithm(Default::default()));
/// }
/// ```
#[macro_export]
macro_rules! start {
    ($ipc:expr, $log:expr, $alg: expr) => {{
        use $crate::ipc::Blocking;
        $crate::start!($ipc, $log, $alg, Blocking)
    }};
    ($ipc:expr, $log:expr, $alg:expr, $blk:ty) => {{
        $crate::start!($ipc, $log, $alg, $blk, "portus")
    }};
    ($ipc:expr, $alg:expr, $blk:ty, $bindaddr:expr) => {{
        use $crate::ipc::BackendBuilder;
        match $ipc {
            "unix" => {
                use $crate::ipc::unix::Socket;
                // 0,0 for default sndbuf and rcvbuf size
                let b = Socket::<$blk>::new($bindaddr, 0, 0)
                    .map(|sk| BackendBuilder { sock: sk })
                    .expect("ipc initialization");
                $crate::RunBuilder::new(b).default_alg($alg).run()
            }
            #[cfg(all(target_os = "linux"))]
            "netlink" => {
                use $crate::ipc::netlink::Socket;
                let b = Socket::<$blk>::new()
                    .map(|sk| BackendBuilder { sock: sk })
                    .expect("ipc initialization");
                $crate::RunBuilder::new(b).default_alg($alg).run()
            }
            #[cfg(all(target_os = "linux"))]
            "char" => {
                use $crate::ipc::kp::Socket;
                let b = Socket::<$blk>::new()
                    .map(|sk| BackendBuilder { sock: sk })
                    .expect("ipc initialization");
                $crate::RunBuilder::new(b).default_alg($alg).run()
            }
            _ => unreachable!(),
        }
    }};
}
