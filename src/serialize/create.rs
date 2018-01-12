use std;
use std::io::prelude::*;
use Result;
use super::{AsRawMsg, RawMsg, HDR_LENGTH, u32_to_u8s};

pub(crate) const CREATE: u8 = 0;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Msg {
    pub sid: u32,
    pub init_cwnd: u32,
    pub mss: u32,
    pub src_ip: u32,
    pub src_port: u32,
    pub dst_ip: u32,
    pub dst_port: u32,
    pub cong_alg: String,
}

impl AsRawMsg for Msg {
    fn get_hdr(&self) -> (u8, u8, u32) {
        (
            CREATE,
            HDR_LENGTH + 6 * 4 + self.cong_alg.len() as u8,
            self.sid,
        )
    }

    fn get_u32s<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, self.init_cwnd as u32);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.mss as u32);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.src_ip as u32);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.src_port as u32);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.dst_ip as u32);
        w.write_all(&buf[..])?;
        u32_to_u8s(&mut buf, self.dst_port as u32);
        w.write_all(&buf[..])?;
        Ok(())
    }

    fn get_u64s<W: Write>(&self, _: &mut W) -> Result<()> {
        Ok(())
    }

    fn get_bytes<W: Write>(&self, w: &mut W) -> Result<()> {
        w.write_all(self.cong_alg.clone().as_bytes())?;
        Ok(())
    }

    fn from_raw_msg(msg: RawMsg) -> Result<Self> {
        let u32s = unsafe { msg.get_u32s() }?;
        let b = msg.get_bytes()?;
        let s = std::str::from_utf8(b)?;
        let alg = String::from(s);
        Ok(Msg {
            sid: msg.sid,
            init_cwnd: u32s[0],
            mss: u32s[1],
            src_ip: u32s[2],
            src_port: u32s[3],
            dst_ip: u32s[4],
            dst_port: u32s[5],
            cong_alg: alg,
        })
    }
}
