//! Message sent from datapath to CCP when it starts up, indicating its address

use super::{u32_to_u8s, AsRawMsg, RawMsg, HDR_LENGTH};
use crate::Result;
use std::io::prelude::*;

pub(crate) const READY: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Msg {
    pub id: u32,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (READY, HDR_LENGTH + 1 * 4, 0)
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.id as u32);
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_bytes<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let u32s = unsafe { msg.get_u32s() }?;
        Ok(Msg {
            id: u32s[0]
        })
    }
}

#[cfg(test)]
mod tests {
    macro_rules! check_ready_msg {
        ($id: ident, $msg: expr) => {
            check_msg!($id, super::Msg, $msg, crate::serialize::Msg::Rdy(rdym), rdym);
        };
    }

    check_ready_msg!(
        test_ready_1,
        super::Msg {
            id: 7,
        }
    );

    extern crate test;
    use self::test::Bencher;

    #[bench]
    fn bench_flip_ready(b: &mut Bencher) {
        b.iter(|| test_ready_1())
    }
}
