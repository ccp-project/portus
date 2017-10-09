use std::sync::{Arc, Mutex};
use super::ipc::*;
use super::serialize::*;

#[derive(Clone)]
struct FakeIpc(Arc<Mutex<Vec<u8>>>);

impl FakeIpc {
    fn new() -> Self {
        FakeIpc(Arc::new(Mutex::new(Vec::new())))
    }
}

impl Ipc for FakeIpc {
    fn send(&self, _: Option<u16>, msg: &[u8]) -> Result<(), super::ipc::Error> {
        let mut x = self.0.lock().unwrap();
        (*x).extend(msg);
        Ok(())
    }

    // return the number of bytes read if successful.
    fn recv(&self, mut msg: &mut [u8]) -> Result<usize, super::ipc::Error> {
        use std::io::Write;
        use std::cmp;
        let x = self.0.lock().unwrap();
        let w = cmp::min(msg.len(), (*x).len());
        msg.write_all(&(*x)[0..w]).expect("fakeipc write to recv buffer");
        Ok(w)
    }

    fn close(&self) -> Result<(), super::ipc::Error> {
        Ok(())
    }
}


// this doesn't work on Darwin currently. Not sure why.
#[cfg(not(target_os="macos"))]
#[test]
fn test_ser_over_ipc() {
    use std;
    use std::thread;

    let (tx, rx) = std::sync::mpsc::channel();
    let sk = FakeIpc::new();
    let sk1 = sk.clone();
    let c1 = thread::spawn(move || {
        let (_, r1) = Backend::new(sk1).expect("init backend");
        tx.send(true).expect("ready chan send");
        let mut msg = r1.recv().expect("receive message"); // Vec<u8>

        // deserialize the message
        if msg.len() <= 6 {
            panic!("msg too small: {:?}", msg);
        }

        let raw_msg = deserialize(&mut msg[..]).expect("deserialization");
        let got = Msg::get(raw_msg).expect("typing");

        assert_eq!(got,
                   Msg::Cr(CreateMsg {
                       sid: 42,
                       start_seq: 42,
                       cong_alg: String::from("foobar"),
                   }));
    });

    let sk2 = sk.clone();
    let c2 = thread::spawn(move || {
        rx.recv().expect("ready chan rcv");
        let (b2, _) = Backend::new(sk2).expect("init backend");

        // serialize a message
        let m = CreateMsg {
            sid: 42,
            start_seq: 42,
            cong_alg: String::from("foobar"),
        };

        let buf = RMsg::<CreateMsg>(m.clone()).serialize().expect("serialize");
        b2.send_msg(None, &buf[..]).expect("send message");
    });

    c2.join().expect("join sender thread");
    c1.join().expect("join rcvr thread");
}

extern crate test;
use self::test::Bencher;

#[bench]
fn bench_ser_over_ipc(b: &mut Bencher) {
    use std;
    use std::thread;

    let (exp_start_tx, exp_start_rx) = std::sync::mpsc::channel();
    let (exp_end_tx, exp_end_rx) = std::sync::mpsc::channel();
    let (tx, rx) = std::sync::mpsc::channel();
    let sk = FakeIpc::new();
    let sk1 = sk.clone();
    thread::spawn(move || {
        let (_, r1) = Backend::new(sk1).expect("init backend");
        loop {
            tx.send(true).expect("ready chan send");
            let mut msg = r1.recv().expect("receive message"); // Vec<u8>

            // deserialize the message
            if msg.len() <= 6 {
                panic!("msg too small: {:?}", msg);
            }

            let raw_msg = deserialize(&mut msg[..]).expect("deserialization");
            let got = Msg::get(raw_msg).expect("typing");

            assert_eq!(got,
                       Msg::Cr(CreateMsg {
                           sid: 42,
                           start_seq: 42,
                           cong_alg: String::from("foobar"),
                       }));

            if let Err(_) = exp_end_tx.send(true) {
                break;
            }
        }
    });

    let sk2 = sk.clone();
    thread::spawn(move || {
        let (b2, _) = Backend::new(sk2).expect("init backend");
        let m = CreateMsg {
            sid: 42,
            start_seq: 42,
            cong_alg: String::from("foobar"),
        };

        loop {
            let cont = exp_start_rx.recv().expect("exp start");
            if cont == false {
                break;
            }

            rx.recv().expect("ready chan rcv");

            // send a message
            let buf = RMsg::<CreateMsg>(m.clone()).serialize().expect("serialize");
            b2.send_msg(None, &buf[..]).expect("send message");
        }
    });

    b.iter(|| {
        exp_start_tx.send(true).expect("next iter send");
        exp_end_rx.recv().expect("test iter end");
    });

    exp_start_tx.send(false).expect("shutdown");
}
