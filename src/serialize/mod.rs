use std;
use std::vec::Vec;
use std::io::prelude::*;
use std::io::Cursor;

use super::Result;

use bytes::{ByteOrder, LittleEndian};

fn u16_to_u8s(buf: &mut [u8], num: u16) {
    LittleEndian::write_u16(buf, num);
}

pub(crate) fn u32_to_u8s(buf: &mut [u8], num: u32) {
    LittleEndian::write_u32(buf, num);
}

pub(crate) fn u64_to_u8s(buf: &mut [u8], num: u64) {
    LittleEndian::write_u64(buf, num);
}

fn u16_from_u8s(buf: &[u8]) -> u16 {
    LittleEndian::read_u16(buf)
}

pub(crate) fn u32_from_u8s(buf: &[u8]) -> u32 {
    LittleEndian::read_u32(buf)
}

pub(crate) fn u64_from_u8s(buf: &[u8]) -> u64 {
    LittleEndian::read_u64(buf)
}

/// (`type`, `len`, `socket_id`) header
/// -----------------------------------
/// | Msg Type | Len (B)  | Uint32    |
/// | (2 B)    | (2 B)    | (32 bits) |
/// -----------------------------------
/// total: 8 Bytes
///
pub const HDR_LENGTH: u32 = 8;
fn serialize_header(typ: u8, len: u32, sid: u32) -> Vec<u8> {
    let mut hdr = [0u8; 8];
    u16_to_u8s(&mut hdr[0..2], u16::from(typ));
    u16_to_u8s(&mut hdr[2..4], len as u16);
    u32_to_u8s(&mut hdr[4..], sid);
    hdr.to_vec()
}

fn deserialize_header<R: Read>(buf: &mut R) -> Result<(u8, u32, u32)> {
    let mut hdr = [0u8; 8];
    buf.read_exact(&mut hdr)?;
    let typ = u16_from_u8s(&hdr[0..2]);
    let len = u16_from_u8s(&hdr[2..4]);
    let sid = u32_from_u8s(&hdr[4..]);

    Ok((typ as u8, u32::from(len), sid))
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct RawMsg<'a> {
    pub typ: u8,
    pub len: u32,
    pub sid: u32,
    bytes: &'a [u8],
}

impl<'a> RawMsg<'a> {
    /// For predefined messages, get u32s separately for convenience
    pub(crate) unsafe fn get_u32s(&self) -> Result<&'a [u32]> {
        use std::mem;
        match self.typ {
            create::CREATE => Ok(mem::transmute(&self.bytes[0..(4 * 6)])),
            measure::MEASURE | update_field::UPDATE_FIELD => Ok(mem::transmute(&self.bytes[0..4])),
            _ => Ok(&[]),
        }
    }

    /// For predefined messages, bytes blob is whatever's left (may be nothing)
    /// For other message types, just return the bytes blob
    pub fn get_bytes(&self) -> Result<&'a [u8]> {
        match self.typ {
            measure::MEASURE | update_field::UPDATE_FIELD => Ok(&self.bytes[4..(self.len as usize - HDR_LENGTH as usize)]),
            _ => Ok(self.bytes),
        }
    }
}

/// A message type has 4 components, always in the following order.
/// 1. Header
/// 2. u32s
/// 3. u64s
/// 4. Arbitrary bytes
///
/// For convenience, the predefined message types define a number of u32s and u64s.
/// External message types can implement `get_bytes()` to pass custom types in the message payload.
/// In these cases, there is no overhead from the u32 and u64 parts of the message.
/// Message types wanting to become "predefined" (and as such take advantage of `get_u32s()` and
/// `get_u64s()` below) should edit this file accordingly (see `impl RawMsg`)
pub trait AsRawMsg {
    fn get_hdr(&self) -> (u8, u32, u32);
    fn get_u32s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()>;
    fn from_raw_msg(msg: RawMsg) -> Result<Self>
    where
        Self: std::marker::Sized;
}

#[macro_use]
mod test_helper {
    /// Generates a test which serializes and deserializes a message 
    /// and verifies the message is unchanged.
    #[macro_export]
    macro_rules! check_msg {
        ($id: ident, $typ: ty, $m: expr, $got: pat, $x: ident) => (
            #[test]
            fn $id() {
                let m = $m;
                let buf: Vec<u8> = ::serialize::serialize::<$typ>(&m.clone()).expect("serialize");
                let msg = ::serialize::Msg::from_buf(&buf[..]).expect("deserialize");
                match msg {
                    $got => assert_eq!($x, m),
                    _ => panic!("wrong type for message"),
                }
            }
        )
    }
}

pub mod create;
pub mod measure;
pub mod install;
pub mod update_field;
mod testmsg;

pub fn serialize<T: AsRawMsg>(m: &T) -> Result<Vec<u8>> {
    let (a, b, c) = m.get_hdr();
    let mut msg = serialize_header(a, b, c);
    m.get_u32s(&mut msg)?;
    m.get_u64s(&mut msg)?;
    m.get_bytes(&mut msg)?;
    Ok(msg)
}

fn deserialize(buf: &[u8]) -> Result<RawMsg> {
    let mut buf = Cursor::new(buf);
    let (typ, len, sid) = deserialize_header(&mut buf)?;
    let i = buf.position();
    Ok(RawMsg {
        typ,
        len,
        sid,
        bytes: &buf.into_inner()[i as usize..],
    })
}

/// Message type for deserialization.
/// Reads message type in the header of the input buffer and returns
/// a Msg of the corresponding type. If the message type is unkown, returns a
/// wrapper with direct access to the message bytes.
#[derive(Debug)]
#[derive(PartialEq)]
pub enum Msg<'a> {
    Cr(create::Msg),
    Ms(measure::Msg),
    Ins(install::Msg),
    Other(RawMsg<'a>),
}

impl<'a> Msg<'a> {
    fn from_raw_msg(m: RawMsg) -> Result<Msg> {
        match m.typ {
            create::CREATE => Ok(Msg::Cr(create::Msg::from_raw_msg(m)?)),
            measure::MEASURE => Ok(Msg::Ms(measure::Msg::from_raw_msg(m)?)),
            install::INSTALL => Ok(Msg::Ins(install::Msg::from_raw_msg(m)?)),
            update_field::UPDATE_FIELD => unimplemented!(),
            _ => Ok(Msg::Other(m)),
        }
    }

    pub fn from_buf(buf: &[u8]) -> Result<Msg> {
        deserialize(buf).and_then(Msg::from_raw_msg)
    }
}

#[cfg(test)]
mod tests {
    use super::Msg;

    #[test]
    fn test_from_u16() {
        let mut buf = [0u8; 2];
        let x: u16 = 2;
        super::u16_to_u8s(&mut buf, x);
        assert_eq!(buf, [0x2, 0x0]);
    }

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
    fn test_to_u16() {
        let buf = vec![0x3, 0];
        let x = super::u16_from_u8s(&buf[..]);
        assert_eq!(x, 3);
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
}
