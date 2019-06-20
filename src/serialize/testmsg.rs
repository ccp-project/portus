use super::{AsRawMsg, RawMsg, HDR_LENGTH};
use crate::Result;
use std;
use std::io::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Msg(pub String);

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (0xff, HDR_LENGTH + self.0.len() as u32, 0)
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(self.0.clone().as_bytes())?;
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let b = msg.get_bytes()?;
        let s = std::str::from_utf8(b)?;
        let st = String::from(s);
        Ok(Msg(st))
    }
}
#[cfg(test)]
impl Msg {
    pub fn from_other_msg(msg: super::other::Msg) -> Result<Self> {
        let b = msg.get_raw_bytes();
        let s = std::str::from_utf8(b)?;
        let st = String::from(s);
        Ok(Msg(st))
    }
}
