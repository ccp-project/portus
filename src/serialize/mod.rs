use std;
use std::mem;
use std::vec::Vec;
use std::io::prelude::*;
use std::io::Cursor;

#[derive(Debug)]
pub struct Error(String);

impl From<String> for Error {
    fn from(e: String) -> Error {
        Error(e)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(format!("err {}", e))
    }
}

type Result<T> = std::result::Result<T, Error>;

macro_rules! to_u8s {
    ($s: ty, $x:expr) => {
        unsafe {
            let p : *const u8 = std::mem::transmute(&$x);
            std::slice::from_raw_parts(p, mem::size_of::<$s>())
        }
    }
}

macro_rules! from_u8s {
    ($s: ty, $x:expr) => (
        *unsafe { 
            let ptr : *const u8 = $x[0..(mem::size_of::<$s>())].as_ptr();
            std::mem::transmute::<*const u8, &$s>(ptr) 
        }
    )
}

/* (type, len, socket_id) header
 * -----------------------------------
 * | Msg Type | Len (B)  | Uint32    |
 * | (1 B)    | (1 B)    | (32 bits) |
 * -----------------------------------
 * total: 6 Bytes
 */
const HDR_LENGTH: u8 = 6;
fn serialize_header(typ: u8, len: u8, sid: u32) -> Vec<u8> {
    let mut hdr = Vec::new();
    hdr.push(typ);
    hdr.push(len);
    hdr.extend(to_u8s!(u32, sid));
    hdr
}

fn deserialize_header<R: Read>(buf: &mut R) -> Result<(u8, u8, u32)> {
    let mut hdr = [0u8; 6];
    buf.read_exact(&mut hdr)?;
    let typ = hdr[0];
    let len = hdr[1];
    let sid = from_u8s!(u32, hdr[2..]);

    Ok((typ, len, sid))
}

pub struct RawMsg<'a> {
    typ: u8,
    len: u8,
    sid: u32,
    bytes: &'a [u8],
}

impl<'a> RawMsg<'a> {
    pub unsafe fn get_u32s(&self) -> Result<&'a [u32]> {
        use std::mem;
        match self.typ {
            CREATE => Ok(mem::transmute(&self.bytes[0..4])),
            MEASURE => Ok(mem::transmute(&self.bytes[0..4 * 2])),
            DROP => Ok(&[]),
            CWND => Ok(&[]),
            _ => Err(Error(String::from("malformed msg"))),
        }
    }

    pub unsafe fn get_u64s(&self) -> Result<&'a [u64]> {
        use std::mem;
        match self.typ {
            CREATE => Ok(&[]),
            MEASURE => Ok(mem::transmute(&self.bytes[(4 * 2)..(4 * 2 + 8 * 2)])),
            DROP => Ok(&[]),
            CWND => Ok(&[]),
            _ => Err(Error(String::from("malformed msg"))),
        }
    }

    pub fn get_bytes(&self) -> Result<&'a [u8]> {
        match self.typ {
            CREATE => Ok(&self.bytes[4..(self.len as usize - 6)]),
            MEASURE => Ok(&[]),
            DROP => Ok(&self.bytes[0..(self.len as usize - 6)]),
            CWND => Ok(&self.bytes[0..(self.len as usize - 6)]),
            _ => Err(Error(String::from("malformed msg"))),
        }
    }
}

pub trait AsRawMsg {
    fn get_hdr(&self) -> (u8, u8, u32);
    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()>;
    fn get_u64s<W: Write>(&self, w: &mut W) -> Result<()>;
    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()>;

    fn from_raw_msg(msg: RawMsg) -> Result<Self> where Self: std::marker::Sized;
}

pub struct RMsg<T: AsRawMsg>(T);

impl<T: AsRawMsg> RMsg<T> {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let (a, b, c) = self.0.get_hdr();
        let mut msg = serialize_header(a, b, c);
        self.0.get_u32s(&mut msg)?;
        self.0.get_u64s(&mut msg)?;
        self.0.get_bytes(&mut msg)?;
        Ok(msg)
    }
}

const CREATE: u8 = 0;
#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct CreateMsg {
    sid: u32,
    start_seq: u32,
    cong_alg: String,
}

impl AsRawMsg for CreateMsg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (CREATE, HDR_LENGTH + 4 + self.cong_alg.len() as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(to_u8s!(u32, self.start_seq))?;
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(self.cong_alg.clone().as_bytes())?;
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let b = msg.get_bytes()?;
        let s = std::str::from_utf8(b)?;
        let alg = String::from(s);
        Ok(CreateMsg {
            sid: msg.sid,
            start_seq: unsafe { msg.get_u32s() }?[0],
            cong_alg: alg,
        })
    }
}

const MEASURE: u8 = 1;
#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct MeasureMsg {
    pub sid: u32,
    pub ack: u32,
    pub rtt_us: u32,
    pub rin: u64,
    pub rout: u64,
}

impl AsRawMsg for MeasureMsg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (MEASURE, HDR_LENGTH + 8 + 16 as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(to_u8s!(u32, self.ack))?;
        w.write_all(to_u8s!(u32, self.rtt_us))?;
        Ok(())
    }

    fn get_u64s<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(to_u8s!(u64, self.rin))?;
        w.write_all(to_u8s!(u64, self.rout))?;
        Ok(())
    }

    fn get_bytes<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let u32s = unsafe { msg.get_u32s() }?;
        let u64s = unsafe { msg.get_u64s() }?;
        Ok(MeasureMsg {
            sid: msg.sid,
            ack: u32s[0],
            rtt_us: u32s[1],
            rin: u64s[0],
            rout: u64s[1],
        })
    }
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct DropMsg {
    sid: u32,
    event: String,
}

const DROP: u8 = 2;
impl AsRawMsg for DropMsg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (DROP, HDR_LENGTH + self.event.len() as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(self.event.clone().as_bytes())?;
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let b = msg.get_bytes()?;
        let s = std::str::from_utf8(b)?;
        let ev = String::from(s);
        Ok(DropMsg {
            sid: msg.sid,
            event: ev,
        })
    }
}

#[macro_use]
mod pattern;
#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct PatternMsg {
    sid: u32,
    pattern: pattern::Pattern,
}

const CWND: u8 = 3;
impl AsRawMsg for PatternMsg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (CWND, HDR_LENGTH + self.pattern.len_bytes() as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        self.pattern.serialize(w)?;
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let mut b = msg.get_bytes()?;
        Ok(PatternMsg {
            sid: msg.sid,
            pattern: pattern::Pattern::deserialize(&mut b)?,
        })
    }
}

pub fn deserialize(buf: &[u8]) -> Result<RawMsg> {
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

pub enum Msg {
    Cr(CreateMsg),
    Dr(DropMsg),
    Ms(MeasureMsg),
    Pt(PatternMsg),
}

impl Msg {
    pub fn get(m: RawMsg) -> Result<Msg> {
        match m.typ {
            CREATE => Ok(Msg::Cr(CreateMsg::from_raw_msg(m)?)),
            DROP => Ok(Msg::Dr(DropMsg::from_raw_msg(m)?)),
            MEASURE => Ok(Msg::Ms(MeasureMsg::from_raw_msg(m)?)),
            CWND => Ok(Msg::Pt(PatternMsg::from_raw_msg(m)?)),
            _ => Err(Error(String::from("unknown type"))),
        }
    }
}

#[cfg(test)]
mod test;
