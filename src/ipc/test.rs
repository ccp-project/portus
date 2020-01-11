use super::Ipc;
use std::sync::{Arc, Mutex};

use crate::ipc::Backend;

#[derive(Clone)]
pub struct FakeIpc(Arc<Mutex<Vec<u8>>>);

impl FakeIpc {
    pub fn new() -> Self {
        FakeIpc(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Ipc for FakeIpc {
    fn name() -> String {
        String::from("fake")
    }

    fn send(&self, msg: &[u8]) -> Result<(), super::Error> {
        let mut x = self.0.lock().unwrap();
        (*x).extend(msg);
        Ok(())
    }

    // return the number of bytes read if successful.
    fn recv(&self, msg: &mut [u8]) -> super::Result<usize> {
        use std::cmp;
        let x = self.0.lock().unwrap();
        let w = cmp::min(msg.len(), (*x).len());
        let dest_slice = &mut msg[0..w];
        dest_slice.copy_from_slice(&(*x)[0..w]);
        Ok(w)
    }

    fn close(&mut self) -> Result<(), super::Error> {
        Ok(())
    }
}

#[test]
fn test_unix() {
    use super::Blocking;
    use crate::serialize;
    use crate::serialize::Msg;
    use crate::test_helper::TestMsg;
    use std::sync::atomic;
    use std::thread;

    let (tx, rx) = crossbeam::channel::unbounded();

    let c2 = thread::spawn(move || {
        rx.recv().expect("chan rcv");
        let sk2 = super::unix::Socket::<Blocking>::new(1, "out", "in").expect("init socket");
        let b2 = super::SingleBackend::new(sk2, Arc::new(atomic::AtomicBool::new(true)));
        let test_msg = TestMsg(String::from("hello, world"));
        let test_msg_buf = serialize::serialize(&test_msg).expect("serialize test msg");
        b2.sender()
            .send_msg(&test_msg_buf[..])
            .expect("send message");
    });

    let sk1 = super::unix::Socket::<Blocking>::new(1, "in", "out").expect("init socket");
    let mut b1 = super::SingleBackend::new(sk1, Arc::new(atomic::AtomicBool::new(true)));
    tx.send(true).expect("chan send");
    match b1.next().expect("receive message") {
        // Msg::Other(RawMsg)
        Msg::Other(r) => {
            assert_eq!(r.typ, 0xff);
            assert_eq!(r.len, serialize::HDR_LENGTH + "hello, world".len() as u32);
            assert_eq!(r.get_raw_bytes(), "hello, world".as_bytes());
        }
        _ => unreachable!(),
    }

    c2.join().expect("join sender thread");
}

#[test]
fn test_chan() {
    use super::Blocking;
    use crate::serialize;
    use crate::serialize::Msg;
    use crate::test_helper::TestMsg;
    use std::sync::atomic;
    use std::thread;

    let (tx, rx) = crossbeam::channel::unbounded();

    let (s1, r1) = crossbeam::channel::unbounded();
    let (s2, r2) = crossbeam::channel::unbounded();

    let c2 = thread::spawn(move || {
        rx.recv().expect("chan rcv");
        let sk2 = super::chan::Socket::<Blocking>::new(s1, r2);
        let b2 = super::SingleBackend::new(sk2, Arc::new(atomic::AtomicBool::new(true)));
        let test_msg = TestMsg(String::from("hello, world"));
        let test_msg_buf = serialize::serialize(&test_msg).expect("serialize test msg");
        b2.sender()
            .send_msg(&test_msg_buf[..])
            .expect("send message");
    });

    let sk1 = super::chan::Socket::<Blocking>::new(s2, r1);
    let mut b1 = super::SingleBackend::new(sk1, Arc::new(atomic::AtomicBool::new(true)));
    tx.send(true).expect("chan send");
    match b1.next().expect("receive message") {
        // Msg::Other(RawMsg)
        Msg::Other(r) => {
            assert_eq!(r.typ, 0xff);
            assert_eq!(r.len, serialize::HDR_LENGTH + "hello, world".len() as u32);
            assert_eq!(r.get_raw_bytes(), "hello, world".as_bytes());
        }
        _ => unreachable!(),
    }

    c2.join().expect("join sender thread");
}
