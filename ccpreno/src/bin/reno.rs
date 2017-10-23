extern crate ccpreno;
extern crate portus;
use ccpreno::Reno;
use portus::ipc::Backend;

#[cfg(all(target_os = "linux"))]
fn main() {
    use portus::ipc::netlink::Socket;
    let b = Socket::new().and_then(|sk| Backend::new(sk)).expect(
        "ipc initialization",
    );

    portus::start::<_, Reno<Socket>>(b);
}

#[cfg(not(target_os = "linux"))]
fn main() {
    use portus::ipc::unix::Socket;
    let b = Socket::new(0).and_then(|sk| Backend::new(sk)).expect(
        "ipc initialization",
    );

    portus::start::<_, Reno<Socket>>(b);
}
