use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s};
use ccp_measure_lang::Bin;

pub(crate) const INSTALL_FOLD: u8 = 4;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub num_instrs: u32,
    pub instrs: Bin,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (
            INSTALL_FOLD,
            HDR_LENGTH + 4 + (self.instrs.0.len() * 4) as u8,
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.num_instrs);
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        let buf = self.instrs.serialize()?;
        w.write_all(&buf[..])?;
        Ok(())
    }

    // at least for now, portus doesn't have to worry about deserializing this
    fn from_raw_msg(_msg: RawMsg) -> Result<Self> {
        unimplemented!();
    }
}
