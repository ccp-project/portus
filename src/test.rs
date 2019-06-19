use super::ipc;
use super::serialize;
use std::sync::{atomic, Arc};
use std::thread;

use crate::ipc::Backend;

#[test]
fn test_ser_over_ipc() {
    let (tx, rx) = crossbeam::channel::unbounded();
    let sk = ipc::test::FakeIpc::new();
    let sk1 = sk.clone();
    let c1 = thread::spawn(move || {
        let mut b1 = ipc::SingleBackend::new(sk1, Arc::new(atomic::AtomicBool::new(true)));
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
        let b2 = ipc::SingleBackend::new(sk2, Arc::new(atomic::AtomicBool::new(true)));

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
use crate::ipc::Blocking;

#[bench]
fn bench_ser_over_ipc(b: &mut Bencher) {
    let (s1, r1) = crossbeam::channel::unbounded();
    let (s2, r2) = crossbeam::channel::unbounded();

    let sk1 = ipc::chan::Socket::<Blocking>::new(s1, r2);
    let mut b1 = ipc::SingleBackend::new(sk1, Arc::new(atomic::AtomicBool::new(true)));

    let sk2 = ipc::chan::Socket::<Blocking>::new(s2, r1);
    let b2 = ipc::SingleBackend::new(sk2, Arc::new(atomic::AtomicBool::new(true)));

    let m = serialize::measure::Msg {
        sid: 42,
        program_uid: 12,
        num_fields: 1,
        fields: vec![0],
    };

    b.iter(|| {
        // send a message
        let buf = serialize::serialize(&m.clone()).expect("serialize");
        b2.sender().send_msg(&buf[..]).expect("send message");
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
    });
}
