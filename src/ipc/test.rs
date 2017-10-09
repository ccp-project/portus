use std::sync::{Arc, Mutex};
use super::Ipc;

#[derive(Clone)]
pub struct FakeIpc(Arc<Mutex<Vec<u8>>>);

impl FakeIpc {
    pub fn new() -> Self {
        FakeIpc(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Ipc for FakeIpc {
    fn send(&self, _: Option<u16>, msg: &[u8]) -> Result<(), super::Error> {
        let mut x = self.0.lock().unwrap();
        (*x).extend(msg);
        Ok(())
    }

    // return the number of bytes read if successful.
    fn recv(&self, mut msg: &mut [u8]) -> Result<usize, super::Error> {
        use std::io::Write;
        use std::cmp;
        let x = self.0.lock().unwrap();
        let w = cmp::min(msg.len(), (*x).len());
        msg.write_all(&(*x)[0..w]).expect(
            "fakeipc write to recv buffer",
        );
        Ok(w)
    }

    fn close(&self) -> Result<(), super::Error> {
        Ok(())
    }
}

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
