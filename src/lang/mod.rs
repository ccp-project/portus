use std;
use nom;

#[derive(Debug)]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;
impl From<String> for Error {
    fn from(e: String) -> Error {
        Error(e)
    }
}
impl<'a> From<&'a str> for Error {
    fn from(e: &'a str) -> Error {
        Error(String::from(e))
    }
}
impl From<nom::simple_errors::Err> for Error {
    fn from(e: nom::simple_errors::Err) -> Error {
        Error(String::from(e.description()))
    }
}
impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

mod ast;
mod datapath;
mod serialize;

pub use self::datapath::Bin;
pub use self::datapath::Reg;
pub use self::datapath::Scope;

use self::ast::Prog;

/// `compile()` uses 3 passes to yield Instrs.
///
/// 1. `Expr::new()` (called by `Prog::new_with_scope()` internally) returns a single AST
/// 2. `Prog::new_with_scope()` returns a list of ASTs for multiple expressions
/// 3. `Bin::compile_prog()` turns a `Prog` into a `Bin`, which is a `Vec` of datapath `Instr`
pub fn compile(src: &[u8]) -> Result<(Bin, Scope)> {
    Prog::new_with_scope(src).and_then(|(p, mut s)| Ok((Bin::compile_prog(&p, &mut s)?, s)))
}

/// `compile_and_serialize()` adds a fourth pass.
/// The resulting bytes can be passed to the datapath.
///
/// `serialize::serialize()` serializes a `Bin` into bytes.
pub fn compile_and_serialize(src: &[u8]) -> Result<(Vec<u8>, Scope)> {
    compile(src).and_then(|(b, s)| Ok((b.serialize()?, s)))
}
