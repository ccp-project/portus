// this doesn't work on Darwin currently. Not sure why.
#[cfg(not(target_os = "macos"))]
#[test]
fn test_unix() {
    use std;
    use std::thread;

    let (tx, rx) = std::sync::mpsc::channel();
    let c1 = thread::spawn(move || {
        let sk1 = super::unix::Socket::new(0).expect("init socket");
        let (_, r1) = super::Backend::new(sk1).expect("init backend");
        tx.send(true).expect("chan send");
        let msg = r1.recv().expect("receive message"); // Vec<u8>
        let got = std::str::from_utf8(&msg[..]).expect("parse message to str");
        assert_eq!(got, "hello, world");
    });

    let c2 = thread::spawn(move || {
        rx.recv().expect("chan rcv");
        let sk2 = super::unix::Socket::new(42424).expect("init socket");
        let (b2, _) = super::Backend::new(sk2).expect("init backend");
        b2.send_msg(None, "hello, world".as_bytes()).expect(
            "send message",
        );
    });

    c2.join().expect("join sender thread");
    c1.join().expect("join rcvr thread");
}
