use super::ipc;
use super::serialize;

#[test]
fn test_ser_over_ipc() {
    use std;
    use std::thread;

    let (tx, rx) = std::sync::mpsc::channel();
    let sk = ipc::test::FakeIpc::new();
    let sk1 = sk.clone();
    let c1 = thread::spawn(move || {
        let b1 = ipc::Backend::new(sk1).expect("init backend");
        let r1 = b1.listen();
        tx.send(true).expect("ready chan send");
        let mut msg = r1.recv().expect("receive message"); // Vec<u8>

        // deserialize the message
        if msg.len() <= 6 {
            panic!("msg too small: {:?}", msg);
        }

        let got = serialize::Msg::from_buf(&mut msg[..]).expect("deserialize");
        assert_eq!(
            got,
            serialize::Msg::Cr(serialize::CreateMsg {
                sid: 42,
                start_seq: 42,
                cong_alg: String::from("foobar"),
            })
        );
    });

    let sk2 = sk.clone();
    let c2 = thread::spawn(move || {
        rx.recv().expect("ready chan rcv");
        let b2 = ipc::Backend::new(sk2).expect("init backend");

        // serialize a message
        let m = serialize::CreateMsg {
            sid: 42,
            start_seq: 42,
            cong_alg: String::from("foobar"),
        };

        let buf = serialize::RMsg::<serialize::CreateMsg>(m.clone())
            .serialize()
            .expect("serialize");
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
    let sk = ipc::test::FakeIpc::new();
    let sk1 = sk.clone();
    thread::spawn(move || {
        let b1 = ipc::Backend::new(sk1).expect("init backend");
        let r1 = b1.listen();
        loop {
            tx.send(true).expect("ready chan send");
            let mut msg = r1.recv().expect("receive message"); // Vec<u8>

            // deserialize the message
            if msg.len() <= 6 {
                panic!("msg too small: {:?}", msg);
            }

            let got = serialize::Msg::from_buf(&mut msg[..]).expect("deserialize");
            assert_eq!(
                got,
                serialize::Msg::Cr(serialize::CreateMsg {
                    sid: 42,
                    start_seq: 42,
                    cong_alg: String::from("foobar"),
                })
            );

            if let Err(_) = exp_end_tx.send(true) {
                break;
            }
        }
    });

    let sk2 = sk.clone();
    thread::spawn(move || {
        let b2 = ipc::Backend::new(sk2).expect("init backend");
        let m = serialize::CreateMsg {
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
            let buf = serialize::RMsg::<serialize::CreateMsg>(m.clone())
                .serialize()
                .expect("serialize");
            b2.send_msg(None, &buf[..]).expect("send message");
        }
    });

    b.iter(|| {
        exp_start_tx.send(true).expect("next iter send");
        exp_end_rx.recv().expect("test iter end");
    });

    exp_start_tx.send(false).expect("shutdown");
}
