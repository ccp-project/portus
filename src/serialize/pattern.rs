use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s};

pub(crate) const CWND: u8 = 3;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub num_events: u32,
    pub pattern: ::pattern::Pattern,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (
            CWND,
            HDR_LENGTH + 4 + self.pattern.len_bytes() as u32,
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.num_events);
        w.write_all(&buf[..])?;
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
        let u32s = unsafe { msg.get_u32s() }?;
        let mut b = msg.get_bytes()?;
        Ok(Msg {
            sid: msg.sid,
            num_events: u32s[0],
            pattern: ::pattern::Pattern::deserialize(&mut b)?,
        })
    }
}
