#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
use slog::Drain;

extern crate ccp_dctcp;
extern crate portus;
use ccp_dctcp::Dctcp;
use portus::ipc::Backend;

// TODO update

fn make_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    slog::Logger::root(drain, o!())
}

#[cfg(all(target_os = "linux"))]
fn main() {
    use portus::ipc::netlink::Socket;
    let b = Socket::new().and_then(|sk| Backend::new(sk)).expect(
        "ipc initialization",
    );

    let log = make_logger();
    info!(log, "starting CCP DCTCP");
    portus::start::<_, Dctcp<Socket>>(b, Some(log));
}

#[cfg(not(target_os = "linux"))]
fn main() {
    use portus::ipc::unix::Socket;
    let b = Socket::new(0).and_then(|sk| Backend::new(sk)).expect(
        "ipc initialization",
    );

    let log = make_logger();
    info!(log, "starting CCP DCTCP");
    portus::start::<_, Dctcp<Socket>>(b, Some(log));
}
