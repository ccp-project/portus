//! CCP sends this message specifying that the datapath should set the values of the
//! given fields to the given values.

use super::{u32_to_u8s, u64_to_u8s, AsRawMsg, RawMsg, HDR_LENGTH};
use crate::lang::Reg;
use std::io::prelude::*;
use crate::{Error, Result};

pub(crate) const UPDATE_FIELD: u8 = 3;

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub num_fields: u8,
    pub fields: Vec<(Reg, u64)>,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (
            UPDATE_FIELD,
            HDR_LENGTH + 4 + u32::from(self.num_fields) * 13, // Reg size = 5, u64 size = 8
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, u32::from(self.num_fields));
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 8];
        for f in &self.fields {
            let reg =
                f.0.clone()
                    .into_iter()
                    .map(|e| e.map_err(Error::from))
                    .collect::<Result<Vec<u8>>>()?;
            w.write_all(&reg[..])?;
            u64_to_u8s(&mut buf, f.1);
            w.write_all(&buf[..])?;
        }

        Ok(())
    }

    fn from_raw_msg(_msg: RawMsg) -> Result<Self> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::Reg;

    #[test]
    fn serialize_update_msg() {
        let m = super::Msg {
            sid: 1,
            num_fields: 1,
            fields: vec![(Reg::Implicit(4, crate::lang::Type::Num(None)), 42)],
        };

        let buf: Vec<u8> = crate::serialize::serialize::<super::Msg>(&m.clone()).expect("serialize");
        assert_eq!(
            buf,
            vec![
                3, 0, // UPDATE_FIELD
                25, 0, // length = 25
                1, 0, 0, 0, // sock_id = 1
                1, 0, 0, 0, // num_fields = 1
                2, 4, 0, 0, 0, 0x2a, 0, 0, 0, 0, 0, 0, 0, // Reg::Implicit(4) <- 42
            ],
        );
    }
}
