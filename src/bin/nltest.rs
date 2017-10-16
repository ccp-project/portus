#[cfg(all(target_os = "linux"))] // netlink is linux-only

extern crate portus;
use portus::ipc::Backend;

fn main() {
    use std::process::Command;

    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");

    // make clean
    let mkcl = Command::new("make")
        .arg("clean")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");
    println!("make clean...");
    println!("{}", String::from_utf8_lossy(&mkcl.stdout));

    // compile kernel module
    let mk = Command::new("make")
        .current_dir("./src/ipc/test-nl-kernel")
        .output()
        .expect("make failed to start");
    println!("make...");
    println!("{}", String::from_utf8_lossy(&mk.stdout));

    use std::thread;

    // listen
    let c1 = thread::spawn(move || {
        let b = portus::ipc::netlink::Socket::new()
            .and_then(|sk| Backend::new(sk))
            .expect("ipc initialization");
        let rx = b.listen();
        println!("listen...");
        let msg = rx.recv().expect("receive message");
        let got = std::str::from_utf8(&msg[..]).expect("parse message to str");
        assert_eq!(got, "hello, netlink\0\0"); // word aligned

        println!("send...");
        let msg = "hello, kernel\0\0\0"; // word aligned
        b.send_msg(None, msg.as_bytes()).expect("send response");

        let echo = rx.recv().expect("receive echo");
        let got = std::str::from_utf8(&echo[..]).expect("parse message to str");
        assert_eq!(got, msg);
    });

    // load kernel module
    println!("insmod...");
    Command::new("sudo")
        .arg("insmod")
        .arg("./src/ipc/test-nl-kernel/nltest.ko")
        .output()
        .expect("insmod failed");

    c1.join().expect("join netlink thread");

    println!("rmmod...");
    Command::new("sudo")
        .arg("rmmod")
        .arg("nltest")
        .output()
        .expect("rmmod failed");
    println!("\x1B[32m{}\x1B[0m", "nltest ok");
}
