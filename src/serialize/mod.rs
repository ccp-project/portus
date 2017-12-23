use std;
use std::vec::Vec;
use std::io::prelude::*;
use std::io::Cursor;

use super::Result;

use bytes::{ByteOrder, LittleEndian};

pub(crate) fn u32_to_u8s(buf: &mut [u8], num: u32) {
    LittleEndian::write_u32(buf, num);
}

pub(crate) fn u64_to_u8s(buf: &mut [u8], num: u64) {
    LittleEndian::write_u64(buf, num);
}

pub(crate) fn u32_from_u8s(buf: &[u8]) -> u32 {
    LittleEndian::read_u32(buf)
}

pub(crate) fn u64_from_u8s(buf: &[u8]) -> u64 {
    LittleEndian::read_u64(buf)
}

/// (type, len, socket_id) header
/// -----------------------------------
/// | Msg Type | Len (B)  | Uint32    |
/// | (1 B)    | (1 B)    | (32 bits) |
/// -----------------------------------
/// total: 6 Bytes
///
pub const HDR_LENGTH: u8 = 6;
fn serialize_header(typ: u8, len: u8, sid: u32) -> Vec<u8> {
    let mut hdr = Vec::new();
    hdr.push(typ);
    hdr.push(len);
    let mut buf = [0u8; 4];
    u32_to_u8s(&mut buf, sid);
    hdr.extend(&buf[..]);
    hdr
}

fn deserialize_header<R: Read>(buf: &mut R) -> Result<(u8, u8, u32)> {
    let mut hdr = [0u8; 6];
    buf.read_exact(&mut hdr)?;
    let typ = hdr[0];
    let len = hdr[1];
    let sid = u32_from_u8s(&hdr[2..]);

    Ok((typ, len, sid))
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct RawMsg<'a> {
    pub typ: u8,
    pub len: u8,
    pub sid: u32,
    bytes: &'a [u8],
}

impl<'a> RawMsg<'a> {
    /// For predefined messages, get u32s separately for convenience
    pub(crate) unsafe fn get_u32s(&self) -> Result<&'a [u32]> {
        use std::mem;
        match self.typ {
            create::CREATE => Ok(mem::transmute(&self.bytes[0..4])),
            measure::MEASURE => Ok(mem::transmute(&self.bytes[0..4 * 3])),
            pattern::CWND => Ok(mem::transmute(&self.bytes[0..4])),
            _ => Ok(&[]),
        }
    }

    /// For predefined messages, get u64s separately for convenience
    //pub(crate) unsafe fn get_u64s(&self) -> Result<&'a [u64]> {
    //    use std::mem;
    //    match self.typ {
    //        create::CREATE => Ok(&[]),
    //        measure::MEASURE => Ok(mem::transmute(&self.bytes[(4 * 3)..(4 * 3 + 8 * 2)])),
    //        pattern::CWND => Ok(&[]),
    //        _ => Ok(&[]),
    //    }
    //}
    /// For predefined messages, bytes blob is whatever's left (may be nothing)
    /// For other message types, just return the bytes blob
    pub fn get_bytes(&self) -> Result<&'a [u8]> {
        match self.typ {
            create::CREATE | measure::MEASURE | pattern::CWND => {
                Ok(&self.bytes[4..(self.len as usize - 6)])
            }
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
/// Message types wanting to become "predefined" (and as such take advantage of get_u32s and
/// get_u64s below) should edit this file accordingly (see `impl RawMsg`)
pub trait AsRawMsg {
    fn get_hdr(&self) -> (u8, u8, u32);
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

pub(crate) mod create;
pub(crate) mod measure;
pub(crate) mod pattern;
pub(crate) mod install_fold;
mod testmsg;

pub fn serialize<T: AsRawMsg>(m: T) -> Result<Vec<u8>> {
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
        typ: typ,
        len: len,
        sid: sid,
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
    Pt(pattern::Msg),
    Fld(install_fold::Msg),
    Other(RawMsg<'a>),
}

impl<'a> Msg<'a> {
    fn from_raw_msg(m: RawMsg) -> Result<Msg> {
        match m.typ {
            create::CREATE => Ok(Msg::Cr(create::Msg::from_raw_msg(m)?)),
            measure::MEASURE => Ok(Msg::Ms(measure::Msg::from_raw_msg(m)?)),
            pattern::CWND => Ok(Msg::Pt(pattern::Msg::from_raw_msg(m)?)),
            install_fold::INSTALL_FOLD => Ok(Msg::Fld(install_fold::Msg::from_raw_msg(m)?)),
            _ => Ok(Msg::Other(m)),
        }
    }

    pub fn from_buf(buf: &[u8]) -> Result<Msg> {
        deserialize(buf).and_then(Msg::from_raw_msg)
    }
}

#[cfg(test)]
mod test;
