#![cfg_attr(feature = "bench", feature(test))]

use std::collections::HashMap;

extern crate bytes;
extern crate libc;
extern crate nix;

#[macro_use]
pub mod pattern;
pub mod ipc;
pub mod serialize;

use ipc::Ipc;
use ipc::Backend;
use serialize::Msg;

#[derive(Debug)]
pub struct Error(pub String);

impl From<String> for Error {
    fn from(e: String) -> Error {
        Error(e)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(format!("err {}", e))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl<T: Ipc> Backend<T> {
    // Algorithm implementations use send_pattern() to control the datapath's behavior by
    // calling send_pattern() with:
    // 1. An initialized backend b. See note in start() for ownership.
    // 2. The flow's sock_id. IPC implementations supporting addressing (e.g. Unix sockets, which can
    // communicate with many applications using UDP datapaths)  MUST make the address be sock_id
    // 3. The control pattern prog to install. Implementations can create patterns using make_pattern!.
    // send_pattern() will return quickly with a Result indicating whether the send was successful.
    pub fn send_pattern(&self, sock_id: u32, prog: pattern::Pattern) -> Result<()> {
        let msg = serialize::pattern::Msg {
            sid: sock_id,
            num_events: prog.len() as u32,
            pattern: prog,
        };

        let buf = serialize::serialize(msg)?;
        self.send_msg(Some(sock_id as u16), &buf[..])?;
        Ok(())
    }
}

pub struct Measurement {
    pub ack: u32,
    pub rtt_us: u32,
    pub rate_sent: u64,
    pub rate_achieved: u64,
}

macro_rules! drop_dupack_str {
    () => ("dupack\0")
}

macro_rules! drop_timeout_str {
    () => ("timeout\0")
}

macro_rules! drop_ecn_str {
    () => ("ecn\0")
}

pub enum DropEvent {
    DupAck,
    Timeout,
    Ecn,
}

pub trait CongAlg<T: Ipc> {
    fn name(&self) -> String;
    fn create(&mut self, control: Backend<T>, sock_id: u32, start_seq: u32, init_cwnd: u32);
    fn measurement(&mut self, sock_id: u32, m: Measurement);
    fn drop(&mut self, sock_id: u32, d: DropEvent);
}

// Main execution loop of ccp for the static pipeline use case.
// Blocks "forever".
// In this use case, an algorithm implementation is a binary which
// 1. Initializes an ipc backend (depending on datapath)
// 2. Calls start(), passing the backend b and CongAlg alg.
// start() takes ownership of b. To use send_pattern() below, clone b first.
//
// start():
// 1. listens for messages from the datapath
// 2. call the appropriate message in alg
//
// start() will never return (-> !). It will panic if:
// 1. It receives an invalid drop notification
// 2. It receives a pattern control message (only a datapath should receive these)
// 3. The IPC channel fails.
pub fn start<T, U>(b: Backend<T>) -> !
where
    T: Ipc,
    U: CongAlg<T> + Default,
{
    let mut flows = HashMap::<u32, U>::new();
    for m in b.listen().iter() {
        if let Ok(msg) = Msg::from_buf(&m[..]) {
            match msg {
                Msg::Cr(c) => {
                    if flows.contains_key(&c.sid) {
                        println!("re-creating already created flow");

                        let alg = flows.get_mut(&c.sid).unwrap();
                        alg.create(b.clone(), c.sid, c.start_seq, 10 * 1460);
                        continue;
                    }

                    let mut alg = U::default();
                    alg.create(b.clone(), c.sid, c.start_seq, 10 * 1460);
                    flows.insert(c.sid, alg);
                }
                Msg::Ms(m) => {
                    if let Some(alg) = flows.get_mut(&m.sid) {
                        alg.measurement(
                            m.sid,
                            Measurement {
                                ack: m.ack,
                                rtt_us: m.rtt_us,
                                rate_sent: m.rin,
                                rate_achieved: m.rout,
                            },
                        )
                    }
                }
                Msg::Dr(d) => {
                    if let Some(alg) = flows.get_mut(&d.sid) {
                        alg.drop(
                            d.sid,
                            match d.event.as_ref() {
                                drop_dupack_str!() => DropEvent::DupAck,
                                drop_timeout_str!() => DropEvent::Timeout,
                                drop_ecn_str!() => DropEvent::Ecn,
                                _ => panic!("Unknown drop event type {}", d.event),
                            },
                        )
                    }
                }
                Msg::Pt(_) => {
                    panic!(
                        "The start() listener should never receive a pattern message, \
                                     since it is on the CCP side."
                    )
                }
                _ => continue,
            }
        }
    }

    panic!("The IPC receive channel closed.");
}

#[cfg(test)]
mod test;
