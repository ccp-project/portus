use std::sync::mpsc;

extern crate nix;
extern crate libc;

#[derive(Debug)]
pub struct Error(String);

impl std::convert::From<nix::Error> for Error {
    fn from(e: nix::Error) -> Error {
        Error(format!("err {}", e))
    }
}

pub trait Ipc {
    fn new(addr: Option<u32>) -> Result<(Backend, mpsc::Receiver<Box<[u8]>>), Error>;
    fn send_msg(&self, msg: &[u8]) -> Result<(), Error>;
    fn close(self) -> Result<(), Error>;
}

mod netlink;

pub enum Backend {
    Nl(netlink::Netlink),
}

impl Ipc for Backend {
    fn new(addr: Option<u32>) -> Result<(Self, mpsc::Receiver<Box<[u8]>>), Error> {
        netlink::Netlink::new(addr)
    }

    fn send_msg(&self, msg: &[u8]) -> Result<(), Error> {
        match self {
            &Backend::Nl(ref s) => s.send_msg(msg),
        }
    }

    fn close(self) -> Result<(), Error> {
        match self {
            Backend::Nl(s) => s.close(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
