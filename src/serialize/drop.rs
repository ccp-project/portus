use std;
use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH};

pub(crate) const DROP: u8 = 2;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub event: String,
}

impl AsRawMsg for Msg {
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
        Ok(Msg {
            sid: msg.sid,
            event: ev,
        })
    }
}
