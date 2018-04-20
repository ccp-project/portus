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
            Err(Error(format!("not long enough: {:?}", sl)))
        } else {
            Ok(u64_from_u8s(sl))
        })
        .collect()
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (
            MEASURE,
            HDR_LENGTH + 4 + u32::from(self.num_fields) * 8,
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
            fields: deserialize_fields(b)?,
        })
    }
}

#[cfg(test)]
mod tests {
    macro_rules! check_measure_msg {
        ($id: ident, $sid:expr, $fields:expr) => (
            check_msg!(
                $id,
                super::Msg,
                super::Msg{
                    sid: $sid,
                    num_fields: $fields.len() as u8,
                    fields: $fields,
                },
                ::serialize::Msg::Ms(mes),
                mes
                );
            )
    }

    check_measure_msg!(
        test_measure_1,
        15,
        vec![424242, 65535, 65530, 200000, 150000]
    );
    check_measure_msg!(
        test_measure_2,
        256,
        vec![42424242, 65536, 65531, 100000, 50000]
    );
    check_measure_msg!(
        test_measure_3,
        32,
        vec![
        42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242,42424242
        ]
    );
}
