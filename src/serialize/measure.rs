use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s, u64_to_u8s};

pub(crate) const MEASURE: u8 = 1;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub ack: u32,
    pub rtt_us: u32,
    pub rin: u64,
    pub rout: u64,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (MEASURE, HDR_LENGTH + 8 + 16 as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.ack);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.rtt_us);
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_u64s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 8];
        u64_to_u8s(&mut buf, self.rin);
        w.write_all(&buf[..])?;
        u64_to_u8s(&mut buf, self.rout);
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_bytes<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let u32s = unsafe { msg.get_u32s() }?;
        let u64s = unsafe { msg.get_u64s() }?;
        Ok(Msg {
            sid: msg.sid,
            ack: u32s[0],
            rtt_us: u32s[1],
            rin: u64s[0],
            rout: u64s[1],
        })
    }
}
