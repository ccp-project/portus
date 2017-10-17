use std;
use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s};

pub(crate) const CREATE: u8 = 0;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub start_seq: u32,
    pub cong_alg: String,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (CREATE, HDR_LENGTH + 4 + self.cong_alg.len() as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.start_seq);
        w.write_all(&buf[..])?;
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
        Ok(Msg {
            sid: msg.sid,
            start_seq: unsafe { msg.get_u32s() }?[0],
            cong_alg: alg,
        })
    }
}
