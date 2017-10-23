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

macro_rules! write_event {
    ($t: ident, $buf: ident, $w: ident, $x: expr) => (
        $w.write_all(&[$t, 6])?;
        super::serialize::u32_to_u8s(&mut $buf, $x);
        $w.write_all(&$buf[..])?;
    )
}

impl Event {
    /// Pattern serialization
    /// (type, len, value?) event description
    /// ----------------------------------------
    /// | Event Type | Len (B)  | Uint32?      |
    /// | (1 B)      | (1 B)    | (0||32 bits) |
    /// ----------------------------------------
    /// total: 2 || 6 Bytes
    ///
    pub fn serialize<W: Write>(&self, w: &mut W) -> super::Result<()> {
        let mut buf = [0u8; 4];
        match self {
            &Event::SetCwndAbs(x) => {
                write_event!(SETCWND, buf, w, x);
            }
            &Event::WaitNs(x) => {
                write_event!(WAIT, buf, w, x);
            }
            &Event::SetRateAbs(x) => {
                write_event!(SETRATE, buf, w, x);
            }
            &Event::SetRateRel(f) => {
                write_event!(SETRATEREL, buf, w, (f * 1e3) as u32);
            }
            &Event::WaitRtts(f) => {
                write_event!(WAITREL, buf, w, (f * 1e3) as u32);
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
            let num = super::serialize::u32_from_u8s(&num_buf);
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

/// Convenience macro for creating patterns.
/// Takes event initializations (e.g. pattern::Event::Report())
/// separated by `=>` and creates a Pattern object.
#[macro_export]
macro_rules! make_pattern {
    ($($x: expr)=>*) => ({
        use pattern::Pattern;
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

    pub fn len(&self) -> usize {
        self.0.len()
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
