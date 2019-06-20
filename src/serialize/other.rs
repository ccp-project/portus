//! Message sent from datapath to CCP when a new flow starts.

use super::{AsRawMsg, RawMsg, HDR_LENGTH};
use crate::Result;
use std::io::prelude::*;

pub(crate) const OTHER: u8 = 255;

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub typ: u8,
    pub len: u32,
    pub sid: u32,
    bytes: Vec<u8>,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (OTHER, HDR_LENGTH + self.bytes.len() as u32, self.sid)
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(&self.bytes)?;
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        Ok(Msg {
            typ: msg.typ,
            len: msg.len,
            sid: msg.sid,
            bytes: msg.get_bytes().unwrap().to_vec(),
        })
    }
}

impl Msg {
    pub fn get_raw_bytes(&self) -> &[u8] {
        &self.bytes[..]
    }
}
