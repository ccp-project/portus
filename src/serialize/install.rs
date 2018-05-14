//! CCP sends this message containing a datapath program. 

use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s};
use lang::Bin;

pub(crate) const INSTALL: u8 = 2;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub num_events: u32,
    pub num_instrs: u32,
    pub instrs: Bin,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (
            INSTALL,
            HDR_LENGTH + 8 + (self.num_events * 4 + self.num_instrs * 16),
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.num_events);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.num_instrs);
        w.write_all(&buf[..])?;
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

#[cfg(test)]
mod tests {
    use lang::{Bin, Prog};

    #[test]
    fn serialize_install_msg() {
        let foo = b"
        (def (Report (volatile foo 0)))
        (when true
            (bind Report.foo 4)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let m = super::Msg{
            sid: 1,
            num_events: 1,
            num_instrs: 3,
            instrs: b
        };

        let buf: Vec<u8> = ::serialize::serialize::<super::Msg>(&m.clone()).expect("serialize");
        assert_eq!(
            buf,
            vec![
                2, 0,                                           // INSTALL
                68, 0,                                          // length = 68
                1, 0, 0, 0,                                     // sock_id = 1
                1, 0, 0, 0,                                     // num_events = 1
                3, 0, 0, 0,                                     // num_instrs = 3
                1, 1, 2, 1,                                     // event { flag-idx=1, num-flag=1, body-idx=2, num-body=1 }
                2, 5, 0, 0, 0, 0, 5, 0, 0, 0, 0, 1, 0, 0, 0, 0, // (def (Report.foo 0))
                1, 2, 0, 0, 0, 0, 2, 0, 0, 0, 0, 1, 1, 0, 0, 0, // (when true
                1, 5, 0, 0, 0, 0, 5, 0, 0, 0, 0, 1, 4, 0, 0, 0, //     (bind Report.foo 4))
            ],
        );
    }
}
