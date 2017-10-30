#![feature(box_patterns)]

#[macro_use]
extern crate nom;

#[derive(Debug)]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;
impl From<String> for Error {
    fn from(e: String) -> Error {
        Error(e)
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

pub mod ast;
