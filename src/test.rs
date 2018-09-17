extern crate cluster_message_types;

use std;
use std::thread;
use super::ipc;
use super::serialize;
use std::sync::{Arc, atomic};

use test::cluster_message_types::{allocation::Allocation, summary::Summary};


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
use std::sync::mpsc;
use ipc::Blocking;

#[bench]
fn bench_ser_ipc(b: &mut Bencher) {
    let (s1, r1) = mpsc::channel();
    let (s2, r2) = mpsc::channel();

    let mut buf = [0u8; 1024];
    let sk1 = ipc::chan::Socket::<Blocking>::new(s1, r2).expect("initialize ipc");
    let mut b1 = ipc::Backend::new(
        sk1, 
        Arc::new(atomic::AtomicBool::new(true)), 
        &mut buf[..],
    );

    let mut buf2 = [0u8; 1024];
    let sk2 = ipc::chan::Socket::<Blocking>::new(s2, r1).expect("initialize ipc");
    let b2 = ipc::Backend::new(
        sk2, 
        Arc::new(atomic::AtomicBool::new(true)), 
        &mut buf2[..],
    );

    let m = Allocation {
        id : 23,
        rate: 1234,
        burst: 5678,
        next_summary_in_ms: 42000,
    };
    let mut msg_buf = [0u8; 1024];
    let mut rcv_msg = Allocation::default();

    b.iter(|| {
        // send a message
        m.write_to(&mut msg_buf);
        b2.sender().send_msg(&msg_buf[..]).expect("send message");
        let rcv_buf = b1.get_next_raw().unwrap();   
        rcv_msg.read_from(rcv_buf);
        assert_eq!(
            rcv_msg,
            Allocation {
                id: 23,
                rate: 1234,
                burst: 5678,
                next_summary_in_ms:42000,
            }
        );
    });
}

use ipc::Ipc;

#[bench]
fn bench_ser_ipc_zero_copy(b: &mut Bencher) {
    let (s1, r1) = mpsc::channel();
    let (s2, r2) = mpsc::channel();

    let mut buf = [0u8; 1024];
    let sk1 = ipc::chan::Socket::<Blocking>::new(s1, r2).expect("initialize ipc");
    let mut b1 = ipc::Backend::new(
        sk1, 
        Arc::new(atomic::AtomicBool::new(true)), 
        &mut buf[..],
    );

    let mut buf2 = [0u8; 1024];
    let sk2 = ipc::chan::Socket::<Blocking>::new(s2, r1).expect("initialize ipc");
    let b2 = ipc::Backend::new(
        sk2, 
        Arc::new(atomic::AtomicBool::new(true)), 
        &mut buf2[..],
    );

    let m = Allocation {
        id : 23,
        rate: 1234,
        burst: 5678,
        next_summary_in_ms: 42000,
    };
    let mut rcv_msg = Allocation::default();

    b.iter(|| {

        b2.sender().send_msg(m.as_slice()).expect("send message");
        b1.sock.recv(rcv_msg.as_mut_slice()).expect("recv");

        assert_eq!(
            rcv_msg,
            Allocation {
                id: 23,
                rate: 1234,
                burst: 5678,
                next_summary_in_ms:42000,
            }
        );
    });
}


#[bench]
fn bench_ser(b: &mut Bencher) {


    let m = Allocation {
        id : 23,
        rate: 1234,
        burst: 5678,
        next_summary_in_ms: 42000,
    };
    let mut m2 = Allocation {
        id : 23,
        rate: 1234,
        burst: 5678,
        next_summary_in_ms: 42000,
    };


    b.iter(|| {
        let slice = m.as_slice();
        let rcv = m2.as_mut_slice();
    });
}

#[bench]
fn bench_ser_zero_copy(b: &mut Bencher) {


    let m = serialize::measure::Msg {
        sid: 0,
        program_uid: 10,
        num_fields: 1,
        fields: vec![0],
    };

    let mut m2 = Allocation {
        id : 23,
        rate: 1234,
        burst: 5678,
        next_summary_in_ms: 42000,
    };


    b.iter(|| {
        let buf = serialize::serialize(&m.clone()).expect("serialize");
        let msg = serialize::deserialize(&buf[..]);
    });
}