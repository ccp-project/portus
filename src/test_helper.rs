use std;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct TestMsg(pub String);

use std::io::prelude::*;
use super::serialize;
impl serialize::AsRawMsg for TestMsg {
    fn get_hdr(&self) -> (u8, u32, u32) {
        (0xff, serialize::HDR_LENGTH + self.0.len() as u32, 0)
    }

    fn get_u32s<W: Write>(&self, _: &mut W) -> super::Result<()> {
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> super::Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> super::Result<()> {
        w.write_all(self.0.as_bytes())?;
        Ok(())
    }

    fn from_raw_msg(msg: serialize::RawMsg) -> super::Result<Self> {
        let b = msg.get_bytes()?;
        let got: String = std::str::from_utf8(&b[..])
            .expect("parse message to str")
            .chars()
            .take_while(|b| *b != '\0')
            .collect();
        Ok(TestMsg(got))
    }
}
