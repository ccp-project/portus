use super::Msg;

#[test]
fn test_from_u32() {
    let mut buf = [0u8; 4];
    let x: u32 = 42;
    super::u32_to_u8s(&mut buf, x);
    assert_eq!(buf, [0x2A, 0, 0, 0]);
}

#[test]
fn test_from_u64() {
    let mut buf = [0u8; 8];
    let x: u64 = 42;
    super::u64_to_u8s(&mut buf, x);
    assert_eq!(buf, [0x2A, 0, 0, 0, 0, 0, 0, 0]);

    let x: u64 = 42424242;
    super::u64_to_u8s(&mut buf, x);
    assert_eq!(buf, [0xB2, 0x57, 0x87, 0x02, 0, 0, 0, 0]);
}

#[test]
fn test_to_u32() {
    let buf = vec![0x2A, 0, 0, 0];
    let x = super::u32_from_u8s(&buf[..]);
    assert_eq!(x, 42);

    let buf = vec![0x42, 0, 0x42, 0];
    let x = super::u32_from_u8s(&buf[..]);
    assert_eq!(x, 4325442);
}

#[test]
fn test_to_u64_0() {
    let buf = vec![0x42, 0, 0x42, 0, 0, 0, 0, 0];
    let x = super::u64_from_u8s(&buf[..]);
    assert_eq!(x, 4325442);
}

#[test]
fn test_to_u64_1() {
    let buf = vec![0, 0x42, 0, 0x42, 0, 0x42, 0, 0x42];
    let x = super::u64_from_u8s(&buf[..]);
    assert_eq!(x, 4755873775377990144);
}

macro_rules! check_msg {
    ($id: ident, $typ: ty, $m: expr, $got: pat, $x: ident) => (
        #[test]
        fn $id() {
            let m = $m;
            let buf: Vec<u8> = super::serialize::<$typ>(&m.clone()).expect("serialize");
            let msg = Msg::from_buf(&buf[..]).expect("deserialize");
            match msg {
                $got => assert_eq!($x, m),
                _ => panic!("wrong type for message"),
            }
        }
    )
}

macro_rules! check_create_msg {
    ($id: ident, $sid:expr, $cwnd:expr, $mss:expr, $sip:expr, $sport:expr, $dip:expr, $dport:expr, $alg:expr) => (
        check_msg!(
            $id, 
            super::create::Msg,
            super::create::Msg{
                sid: $sid,
                init_cwnd: $cwnd,
                mss: $mss,
                src_ip: $sip,
                src_port: $sport,
                dst_ip: $dip,
                dst_port: $dport,
                cong_alg: String::from($alg),
            },
            Msg::Cr(crm),
            crm
        );
    )
}

check_create_msg!(
    test_create_1,
    15,
    1448 * 10,
    1448,
    0,
    4242,
    0,
    4242,
    "nimbus"
);

macro_rules! check_measure_msg {
    ($id: ident, $sid:expr, $fields:expr) => (
        check_msg!(
            $id,
            super::measure::Msg,
            super::measure::Msg{
                sid: $sid,
                num_fields: $fields.len() as u8,
                fields: $fields,
            },
            Msg::Ms(mes),
            mes
        );
    )
}

check_measure_msg!(
    test_measure_1,
    15,
    vec![424242, 65535, 65530, 200000, 150000]
);
check_measure_msg!(
    test_measure_2,
    256,
    vec![42424242, 65536, 65531, 100000, 50000]
);
check_measure_msg!(
    test_measure_3,
    32,
    vec![
        42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242
    ]
);

macro_rules! check_pattern_msg {
    ($id: ident, $sid:expr, $p:expr) => (
        check_msg!(
            $id, 
            super::pattern::Msg,
            super::pattern::Msg{
                sid: $sid,
                num_events: $p.len() as u32,
                pattern: $p,
            },
            Msg::Pt(pat),
            pat
        );
    )
}

use pattern;
check_pattern_msg!(test_pattern_1,
                   42,
                   make_pattern!(pattern::Event::Report => pattern::Event::SetCwndAbs(10) => pattern::Event::WaitNs(100)));
check_pattern_msg!(
    test_pattern_2,
    43,
    make_pattern!(pattern::Event::SetRateAbs(1000000) => pattern::Event::WaitRtts(2.0))
);

#[test]
fn test_other_msg() {
    use super::testmsg;
    use super::AsRawMsg;
    let m = testmsg::Msg(String::from("testing"));
    let buf: Vec<u8> = super::serialize::<testmsg::Msg>(&m.clone()).expect("serialize");
    let msg = Msg::from_buf(&buf[..]).expect("deserialize");
    match msg {
        Msg::Other(raw) => {
            let got = testmsg::Msg::from_raw_msg(raw).expect("get raw msg");
            assert_eq!(m, got);
        }
        _ => panic!("wrong type for message"),
    }
}

extern crate test;
use self::test::Bencher;

#[bench]
fn bench_flip_create(b: &mut Bencher) {
    b.iter(|| test_create_1())
}

//#[bench]
//fn bench_flip_measure(b: &mut Bencher) {
//    b.iter(|| test_measure_1())
//}

#[bench]
fn bench_flip_pattern(b: &mut Bencher) {
    b.iter(|| test_pattern_1())
}
