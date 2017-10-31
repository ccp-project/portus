use std;
use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s};

extern crate ccp_measure_lang;
use ccp_measure_lang::Prog;

pub(crate) const INSTALL_FOLD: u8 = 4;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub prog: Prog,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (INSTALL_FOLD, HDR_LENGTH + 4 as u8, self.sid)
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        //let u32s = unsafe { msg.get_u32s() }?;
        //let mut b = msg.get_bytes()?;
        //Ok(Msg {
        //    sid: msg.sid,
        //    num_events: u32s[0],
        //    pattern: ::pattern::Pattern::deserialize(&mut b)?,
        //})
        unimplemented!();
    }
}
