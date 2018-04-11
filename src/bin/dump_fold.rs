extern crate portus;

use std::io::{self, Read};
use portus::{lang, serialize};

fn main() {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();
    println!("buffer: {}", buffer);
    let (bin, _) = lang::compile(buffer.as_bytes()).unwrap();
    let msg = serialize::install::Msg {
        sid: 1,
        num_events: bin.events.len() as u32,
        num_instrs: bin.instrs.len() as u32,
        instrs: bin,
    };

    let buf = serialize::serialize(&msg).unwrap();
    println!("{:?}", buf);
}
