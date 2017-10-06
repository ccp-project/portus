use std;
use std::mem;
use std::vec::Vec;
use std::io::prelude::*;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum Event {
    SetCwndAbs(u32), // bytes
    WaitNs(u32), // ns
    SetRateAbs(u32), // bit/s
    SetRateRel(f32), // no units
    WaitRtts(f32), // no units
    Report, // no units
}

const SETRATE: u8 = 0;
const SETCWND: u8 = 1;
const SETRATEREL: u8 = 2;
const WAIT: u8 = 3;
const WAITREL: u8 = 4;
const REPORT: u8 = 5;

impl Event {
    /* Pattern serialization
     * (type, len, value?) event description
     * ----------------------------------------
     * | Event Type | Len (B)  | Uint32?      |
     * | (1 B)      | (1 B)    | (0||32 bits) |
     * ----------------------------------------
     * total: 2 || 6 Bytes
     */
    pub fn serialize<W: Write>(&self, w: &mut W) -> super::Result<()> {
        match self {
            &Event::SetCwndAbs(x) => {
                w.write_all(&[SETCWND, 6])?;
                w.write_all(to_u8s!(u32, x))?;
            }
            &Event::WaitNs(x) => {
                w.write_all(&[WAIT, 6])?;
                w.write_all(to_u8s!(u32, x))?;
            }
            &Event::SetRateAbs(x) => {
                w.write_all(&[SETRATE, 6])?;
                w.write_all(to_u8s!(u32, x))?;
            }

            &Event::SetRateRel(f) => {
                w.write_all(&[SETRATEREL, 6])?;
                w.write_all(to_u8s!(u32, (f * 1e3) as u32))?;
            }
            &Event::WaitRtts(f) => {
                w.write_all(&[WAITREL, 6])?;
                w.write_all(to_u8s!(u32, (f * 1e3) as u32))?;
            }

            &Event::Report => {
                w.write_all(&[REPORT, 2])?;
            }
        };

        Ok(())
    }

    pub fn deserialize<R: Read>(buf: &mut R) -> super::Result<Self> {
        let mut hdr = [0u8; 2];
        buf.read_exact(&mut hdr)?;
        let typ: u8 = hdr[0];
        let len: u8 = hdr[1];
        if let (REPORT, 2) = (typ, len) {
            Ok(Event::Report)
        } else {
            let mut num_buf = [0u8; 4];
            buf.read_exact(&mut num_buf)?;
            let num = from_u8s!(u32, num_buf);
            match (typ, len) {
                (SETCWND, 6) => Ok(Event::SetCwndAbs(num)),
                (SETRATE, 6) => Ok(Event::SetRateAbs(num)),
                (WAIT, 6) => Ok(Event::WaitNs(num)),
                (SETRATEREL, 6) => Ok(Event::SetRateRel((num as f32) / 1e3)),
                (WAITREL, 6) => Ok(Event::WaitRtts((num as f32) / 1e3)),
                (_, _) => Err(super::Error(String::from("unknown pattern event type"))),
            }
        }
    }
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub struct Pattern(Vec<Event>);

#[macro_export]
macro_rules! make_pattern {
    ($($x: expr)=>*) => ({
        use serialize::pattern::Pattern;
        let mut v = vec![];
        $(
            v.push($x);
        )*
        Pattern::new(v)
    })
}

impl Pattern {
    pub fn new(v: Vec<Event>) -> Self {
        Pattern(v)
    }

    pub fn len_bytes(&self) -> usize {
        self.0
            .iter()
            .map(|e| match e {
                &Event::Report => 2,
                _ => 6,
            })
            .sum()
    }

    pub fn serialize<W: Write>(&self, w: &mut W) -> super::Result<()> {
        let mut buf = vec![];
        for ev in &self.0 {
            ev.serialize(&mut buf)?;
        }

        w.write_all(buf.as_slice())?;

        Ok(())
    }

    pub fn deserialize<R: Read>(r: &mut R) -> super::Result<Self> {
        let mut evs = vec![];
        while let Ok(ev) = Event::deserialize(r) {
            evs.push(ev);
        }

        Ok(Pattern(evs))
    }
}

impl Default for Pattern {
    fn default() -> Self {
        return Pattern(vec![]);
    }
}
