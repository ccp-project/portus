use std;
use std::thread;
use super::ipc;
use super::serialize;
use std::sync::{Arc, atomic};

#[test]
fn test_ser_over_ipc() {
    let (tx, rx) = std::sync::mpsc::channel();
    let sk = ipc::test::FakeIpc::new();
    let sk1 = sk.clone();
    let c1 = thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut b1 = ipc::Backend::new(
            sk1, 
            Arc::new(atomic::AtomicBool::new(true)), 
            &mut buf[..],
        );
        tx.send(true).expect("ready chan send");
        let msg = b1.next().expect("receive message");
        assert_eq!(
            msg,
            serialize::Msg::Ms(serialize::measure::Msg {
                sid: 42,
                program_uid: 7,
                num_fields: 1,
                fields: vec![0],
            })
        );
    });

    let sk2 = sk.clone();
    let c2 = thread::spawn(move || {
        rx.recv().expect("ready chan rcv");
        let mut buf = [0u8; 1024];
        let b2 = ipc::Backend::new(
            sk2, 
            Arc::new(atomic::AtomicBool::new(true)), 
            &mut buf[..],
        );

        // serialize a message
        let m = serialize::measure::Msg {
            sid: 42,
            program_uid: 7,
            num_fields: 1,
            fields: vec![0],
        };

        let buf = serialize::serialize(&m.clone()).expect("serialize");
        b2.sender().send_msg(&buf[..]).expect("send message");
    });

    c2.join().expect("join sender thread");
    c1.join().expect("join rcvr thread");
}

extern crate test;
use self::test::Bencher;

#[bench]
fn bench_ser_over_ipc(b: &mut Bencher) {
    let (exp_start_tx, exp_start_rx) = std::sync::mpsc::channel();
    let (exp_end_tx, exp_end_rx) = std::sync::mpsc::channel();
    let (tx, rx) = std::sync::mpsc::channel();
    let sk = ipc::test::FakeIpc::new();
    let sk1 = sk.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut b1 = ipc::Backend::new(
            sk1, 
            Arc::new(atomic::AtomicBool::new(true)), 
            &mut buf[..],
        );
        loop {
            tx.send(true).expect("ready chan send");
            let msg = b1.next().expect("receive message");
            assert_eq!(
                msg,
                serialize::Msg::Ms(serialize::measure::Msg {
                    sid: 42,
                    program_uid: 12,
                    num_fields: 1,
                    fields: vec![0],
                })
            );

            if let Err(_) = exp_end_tx.send(true) {
                break;
            }
        }
    });

    let sk2 = sk.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let b2 = ipc::Backend::new(
            sk2, 
            Arc::new(atomic::AtomicBool::new(true)), 
            &mut buf[..],
        );
        let m = serialize::measure::Msg {
            sid: 42,
            program_uid: 12,
            num_fields: 1,
            fields: vec![0],
        };

        loop {
            let cont = exp_start_rx.recv().expect("exp start");
            if cont == false {
                break;
            }

            rx.recv().expect("ready chan rcv");

            // send a message
            let buf = serialize::serialize(&m.clone()).expect("serialize");
            b2.sender().send_msg(&buf[..]).expect("send message");
        }
    });

    b.iter(|| {
        exp_start_tx.send(true).expect("next iter send");
        exp_end_rx.recv().expect("test iter end");
    });

    exp_start_tx.send(false).expect("shutdown");
}
