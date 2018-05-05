use std::sync::{Arc, Mutex, atomic};
use super::Ipc;
use ::test_helper::TestMsg;
use ::serialize;
use ::serialize::Msg;

#[derive(Clone)]
pub struct FakeIpc(Arc<Mutex<Vec<u8>>>);

impl FakeIpc {
    pub fn new() -> Self {
        FakeIpc(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Ipc for FakeIpc {
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

    fn recv_nonblocking(&self, msg: &mut [u8]) -> Option<usize> {
        Some(self.recv(msg).unwrap())
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

    let c2 = thread::spawn(move || {
        rx.recv().expect("chan rcv");
        let sk2 = super::unix::Socket::new("out", "in").expect("init socket");
        let mut buf = [0u8; 1024];
        let b2 = super::Backend::new(
            sk2, 
            super::ListenMode::Blocking, 
            Arc::new(atomic::AtomicBool::new(true)), 
            &mut buf[..],
        );
        let test_msg = TestMsg(String::from("hello, world"));
        let test_msg_buf = serialize::serialize(&test_msg).expect("serialize test msg");
        b2.sender().send_msg(&test_msg_buf[..]).expect(
            "send message",
        );
    });
        
    let sk1 = super::unix::Socket::new("in", "out").expect("init socket");
    let mut buf = [0u8; 1024];
    let mut b1 = super::Backend::new(
        sk1, 
        super::ListenMode::Blocking, 
        Arc::new(atomic::AtomicBool::new(true)), 
        &mut buf[..],
    );
    tx.send(true).expect("chan send");
    match b1.next().expect("receive message") { // Msg::Other(RawMsg)
        Msg::Other(r) => {
            assert_eq!(r.typ, 0xff);
            assert_eq!(r.len, serialize::HDR_LENGTH + "hello, world".len() as u32);
            assert_eq!(r.get_bytes().unwrap(), "hello, world".as_bytes());
        }
        _ => unreachable!(),
    }

    c2.join().expect("join sender thread");
}
