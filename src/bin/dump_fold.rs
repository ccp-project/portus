extern crate portus;

use std::io::{self, Read};
use portus::{lang, serialize};

fn main() {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();
    let (bin, _) = lang::compile(buffer.as_bytes()).unwrap();
    let msg = serialize::install_fold::Msg {
        sid: 1,
        num_instrs: bin.0.len() as u32,
        instrs: bin,
    };

    let buf = serialize::serialize(&msg).unwrap();
    println!("{:?}", buf);
}
