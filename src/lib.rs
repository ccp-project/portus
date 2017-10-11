#![cfg_attr(feature = "bench", feature(test))]

extern crate bytes;
extern crate libc;
extern crate nix;

pub mod ipc;
pub mod serialize;

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

#[cfg(test)]
mod test;
