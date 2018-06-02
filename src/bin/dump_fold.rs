extern crate portus;

use std::io::{self, Read};
use portus::{lang, serialize};

/// It is sometimes helpful to deconstruct a datapath program.
/// `dump_fold` is a helper tool for doing so. It accepts datapath 
/// program source from stdin, and outputs to stdout:
/// 0. An echo of the input program.
/// 1. The AST representation of that program
/// 2. The compiled instructions
/// 3. The serialized binary which will be sent to the datapath
///
/// On compilation failure, `dump_fold` will panic with the compilation error.
fn main() {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();
    println!("buffer:\n{}", buffer);
    let (ast, mut sc) = lang::Prog::new_with_scope(buffer.as_bytes()).unwrap();
    println!("ast:\n{:?}", ast);
    let bin = lang::Bin::compile_prog(&ast, &mut sc).unwrap();
    println!("instructions:\n{:?}", bin);
    let msg = serialize::install::Msg {
        sid: 1,
        program_uid: 9,
        num_events: bin.events.len() as u32,
        num_instrs: bin.instrs.len() as u32,
        instrs: bin,
    };

    let buf = serialize::serialize(&msg).unwrap();
    println!("serialized:\n{:?}", buf);
}
