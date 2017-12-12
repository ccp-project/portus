use std::io::prelude::*;
use {Result, Error};
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s, u64_to_u8s, u64_from_u8s};

pub(crate) const MEASURE: u8 = 1;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub num_fields: u8,
    pub fields: Vec<u64>,
}

fn deserialize_fields(buf: &[u8]) -> Result<Vec<u64>> {
    buf.chunks(8)
        .map(|sl| if sl.len() < 8 {
            Err(Error::from(format!("not long enough: {:?}", sl)))
        } else {
            Ok(u64_from_u8s(sl))
        })
        .collect()
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (
            MEASURE,
            HDR_LENGTH + 4 + self.num_fields * 8 as u8,
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.num_fields as u32);
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 8];
        for f in self.fields.iter() {
            u64_to_u8s(&mut buf, *f);
            w.write_all(&buf[..])?;
        }

        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let u32s = unsafe { msg.get_u32s() }?;
        let b = msg.get_bytes()?;
        Ok(Msg {
            sid: msg.sid,
            num_fields: u32s[0] as u8,
            fields: deserialize_fields(&b)?,
        })
    }
}
