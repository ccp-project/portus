//! When the datapath program specifies, the datapath sends a Report message containing
//! measurements to CCP. Use the `Scope` returned from compiling the program to query the values.

use super::{u32_to_u8s, u64_from_u8s, u64_to_u8s, AsRawMsg, RawMsg, HDR_LENGTH};
use std::io::prelude::*;
use {Error, Result};

pub(crate) const MEASURE: u8 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub program_uid: u32,
    // This is actually a u32 in libccp for struct alignment purposes. It *should* be a u8
    // (as it is here), to help enforce the maximum number of fields, but it's much easier
    // to keep everything 4-byte-aligned for de-serialization.
    pub num_fields: u8,
    pub fields: Vec<u64>,
}

fn deserialize_fields(buf: &[u8]) -> Result<Vec<u64>> {
    buf.chunks(8)
        .map(|sl| {
            if sl.len() < 8 {
                Err(Error(format!("not long enough: {:?}", sl)))
            } else {
                Ok(u64_from_u8s(sl))
            }
        })
        .collect()
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (
            MEASURE,
            HDR_LENGTH + 8 + u32::from(self.num_fields) * 8,
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.program_uid);
        w.write_all(&buf[..])?;
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
            program_uid: u32s[0],
            num_fields: u32s[1] as u8,
            fields: deserialize_fields(b)?,
        })
    }
}

#[cfg(test)]
mod tests {
    macro_rules! check_measure_msg {
        ($id: ident, $sid:expr, $program_uid:expr, $fields:expr) => {
            check_msg!(
                $id,
                super::Msg,
                super::Msg {
                    sid: $sid,
                    program_uid: $program_uid,
                    num_fields: $fields.len() as u8,
                    fields: $fields,
                },
                ::serialize::Msg::Ms(mes),
                mes
            );
        };
    }

    check_measure_msg!(
        test_measure_1,
        15,
        72,
        vec![424242, 65535, 65530, 200000, 150000]
    );
    check_measure_msg!(
        test_measure_2,
        256,
        19,
        vec![42424242, 65536, 65531, 100000, 50000]
    );
    check_measure_msg!(
        test_measure_3,
        32,
        3,
        vec![
            42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242,
            42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242,
            42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242,
            42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242, 42424242
        ]
    );
}
