use std;
use std::mem;
use super::{AsRawMsg, RMsg, Msg, CreateMsg, MeasureMsg, DropMsg, PatternMsg, deserialize};

#[test]
fn test_from_u32() {
    let x: u32 = 42;
    let buf: &[u8] = to_u8s!(u32, x);
    assert_eq!(buf, [0x2A, 0, 0, 0]);
}

#[test]
fn test_from_u64() {
    let x: u64 = 42;
    let buf: &[u8] = to_u8s!(u64, x);
    assert_eq!(buf, [0x2A, 0, 0, 0, 0, 0, 0, 0]);

    let x: u64 = 42424242;
    let buf: &[u8] = to_u8s!(u64, x);
    assert_eq!(buf, [0xB2, 0x57, 0x87, 0x02, 0, 0, 0, 0]);
}

#[test]
fn test_to_u32() {
    let buf = vec![0x2A, 0, 0, 0];
    let x = from_u8s!(u32, buf);
    assert_eq!(x, 42);

    let buf = vec![0x42, 0, 0x42, 0];
    let x = from_u8s!(u32, buf);
    assert_eq!(x, 4325442);
}

#[test]
fn test_to_u64_0() {
    let buf = vec![0x42, 0, 0x42, 0, 0, 0, 0, 0];
    let x = from_u8s!(u64, buf);
    assert_eq!(x, 4325442);
}

#[test]
fn test_to_u64_1() {
    let buf = vec![0, 0x42, 0, 0x42, 0, 0x42, 0, 0x42];
    let x = from_u8s!(u64, buf);
    assert_eq!(x, 4755873775377990144);
}

fn flip<T: AsRawMsg>(m: RMsg<T>) -> Msg {
    let mut buf = m.serialize().expect("serialize");
    let raw_msg = deserialize(&mut buf).expect("deserialization");
    Msg::get(raw_msg).expect("typing")
}

macro_rules! check_msg {
    ($id: ident, $typ: ty, $m: expr, $got: pat, $x: ident) => (
        #[test]
        fn $id() {
            let m = $m;
            let msg = flip::<$typ>(RMsg::<$typ>(m.clone()));
            match msg {
                $got => assert_eq!($x, m),
                _ => panic!("wrong type for message"),
            }
        }
    )
}

macro_rules! check_create_msg {
    ($id: ident, $sid:expr, $sseq:expr, $alg:expr) => (
        check_msg!(
            $id, 
            CreateMsg,
            CreateMsg{
                sid: $sid,
                start_seq: $sseq,
                cong_alg: String::from($alg),
            },
            Msg::Cr(crm),
            crm
        );
    )
}

check_create_msg!(test_create_1, 15, 15, "nimbus");
check_create_msg!(test_create_2, 42, 424242, "reno");

macro_rules! check_measure_msg {
    ($id: ident, $sid:expr, $ack:expr, $rtt:expr, $rin:expr, $rout:expr) => (
        check_msg!(
            $id, 
            MeasureMsg,
            MeasureMsg{
                sid: $sid,
                ack: $ack,
                rtt_us: $rtt,
                rin: $rin,
                rout: $rout,
            },
            Msg::Ms(mes),
            mes
        );
    )
}

check_measure_msg!(test_measure_1, 15, 424242, 65535, 200000, 150000);
check_measure_msg!(test_measure_2, 256, 42424242, 65536, 100000, 50000);

macro_rules! check_drop_msg {
    ($id: ident, $sid:expr, $ev:expr) => (
        check_msg!(
            $id, 
            DropMsg,
            DropMsg{
                sid: $sid,
                event: String::from($ev),
            },
            Msg::Dr(drp),
            drp
        );
    )
}

check_drop_msg!(test_drop_1, 15, "TIMEOUT");
check_drop_msg!(test_drop_2, 42, "DUPACK");

macro_rules! check_pattern_msg {
    ($id: ident, $sid:expr, $p:expr) => (
        check_msg!(
            $id, 
            PatternMsg,
            PatternMsg{
                sid: $sid,
                pattern: $p,
            },
            Msg::Pt(pat),
            pat
        );
    )
}

use super::pattern;
check_pattern_msg!(test_pattern_1,
                   42,
                   make_pattern!(pattern::Event::Report => pattern::Event::SetCwndAbs(10) => pattern::Event::WaitNs(100)));
check_pattern_msg!(test_pattern_2,
                   43,
                   make_pattern!(pattern::Event::SetRateAbs(1000000) => pattern::Event::WaitRtts(2.0)));

extern crate test;
use self::test::Bencher;

#[bench]
fn bench_flip_create(b: &mut Bencher) {
    b.iter(|| test_create_1())
}

#[bench]
fn bench_flip_measure(b: &mut Bencher) {
    b.iter(|| test_measure_1())
}

#[bench]
fn bench_flip_drop(b: &mut Bencher) {
    b.iter(|| test_drop_1())
}

#[bench]
fn bench_flip_pattern(b: &mut Bencher) {
    b.iter(|| test_pattern_1())
}
