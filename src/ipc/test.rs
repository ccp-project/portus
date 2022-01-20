use super::Blocking;
use super::Ipc;
use crate::serialize;
use crate::serialize::Msg;
use crate::test_helper::TestMsg;
use std::sync::atomic;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
pub struct FakeIpc(Arc<Mutex<Vec<u8>>>);

impl FakeIpc {
    pub fn new() -> Self {
        FakeIpc(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Ipc for FakeIpc {
    type Addr = ();

    fn name() -> String {
        String::from("fake")
    }

    fn send(&self, msg: &[u8], _to: &Self::Addr) -> Result<(), super::Error> {
        let mut x = self.0.lock().unwrap();
        (*x).extend(msg);
        Ok(())
    }

    // return the number of bytes read if successful.
    fn recv(&self, msg: &mut [u8]) -> super::Result<(usize, Self::Addr)> {
        use std::cmp;
        let x = self.0.lock().unwrap();
        let w = cmp::min(msg.len(), (*x).len());
        let dest_slice = &mut msg[0..w];
        dest_slice.copy_from_slice(&(*x)[0..w]);
        Ok((w, ()))
    }

    fn close(&mut self) -> Result<(), super::Error> {
        Ok(())
    }
}

#[test]
fn test_unix() {
    let (tx, rx) = crossbeam::channel::unbounded();

    let c2 = thread::spawn(move || {
        rx.recv().expect("chan rcv");
        let sk2 = super::unix::Socket::<Blocking>::new("portus-test-unix-1").expect("init socket");
        let mut buf = [0u8; 1024];
        let b2 = super::Backend::new(sk2, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]);
        let test_msg = TestMsg(String::from("hello, world"));
        let test_msg_buf = serialize::serialize(&test_msg).expect("serialize test msg");
        b2.sender(std::path::PathBuf::from("/tmp/ccp/portus-test-unix-2"))
            .send_msg(&test_msg_buf[..])
            .expect("send message");
    });

    let sk1 = super::unix::Socket::<Blocking>::new("portus-test-unix-2").expect("init socket");
    let mut buf = [0u8; 1024];
    let mut b1 = super::Backend::new(sk1, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]);
    tx.send(true).expect("chan send");
    match b1.next().expect("receive message") {
        (Msg::Other(r), _) => {
            assert_eq!(r.typ, 0xff);
            assert_eq!(r.len, serialize::HDR_LENGTH + "hello, world".len() as u32);
            assert_eq!(r.get_bytes().unwrap(), "hello, world".as_bytes());
        }
        _ => unreachable!(),
    }

    c2.join().expect("join sender thread");
}

#[test]
fn test_chan() {
    let (tx, rx) = crossbeam::channel::unbounded();
    let (s1, r1) = crossbeam::channel::unbounded();
    let (s2, r2) = crossbeam::channel::unbounded();

    let c2 = thread::spawn(move || {
        rx.recv().expect("chan rcv");
        let sk2 = super::chan::Socket::<Blocking>::new(s1, r2);
        let mut buf = [0u8; 1024];
        let b2 = super::Backend::new(sk2, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]);
        let test_msg = TestMsg(String::from("hello, world"));
        let test_msg_buf = serialize::serialize(&test_msg).expect("serialize test msg");
        b2.sender(())
            .send_msg(&test_msg_buf[..])
            .expect("send message");
    });

    let sk1 = super::chan::Socket::<Blocking>::new(s2, r1);
    let mut buf = [0u8; 1024];
    let mut b1 = super::Backend::new(sk1, Arc::new(atomic::AtomicBool::new(true)), &mut buf[..]);
    tx.send(true).expect("chan send");
    match b1.next().expect("receive message") {
        // Msg::Other(RawMsg)
        (Msg::Other(r), ()) => {
            assert_eq!(r.typ, 0xff);
            assert_eq!(r.len, serialize::HDR_LENGTH + "hello, world".len() as u32);
            assert_eq!(r.get_bytes().unwrap(), "hello, world".as_bytes());
        }
        _ => unreachable!(),
    }

    c2.join().expect("join sender thread");
}
