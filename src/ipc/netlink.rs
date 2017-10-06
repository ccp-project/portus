extern crate libc;
use libc::c_int;

extern crate nix;
use nix::sys::socket;

#[derive(Debug)]
pub struct Socket(c_int);

impl Socket {
    fn __new() -> Result<Self, nix::Error> {
        let fd = socket::socket(nix::sys::socket::AddressFamily::Netlink,
                                nix::sys::socket::SockType::Raw,
                                nix::sys::socket::SockFlag::empty(),
                                libc::NETLINK_USERSOCK)?;

        let pid = unsafe { libc::getpid() };

        socket::bind(fd, &nix::sys::socket::SockAddr::new_netlink(pid as u32, 0))?;

        Ok(Socket(fd))
    }

    pub fn new(group: u32) -> Result<Self, nix::Error> {
        let s = Self::__new()?;
        s.setsockopt_int(270, libc::NETLINK_ADD_MEMBERSHIP, group as c_int)?;
        Ok(s)
    }

    fn setsockopt_int(&self, level: c_int, option: c_int, val: c_int) -> Result<(), nix::Error> {
        use std::mem;
        let res = unsafe {
            libc::setsockopt(self.0,
                             level,
                             option as c_int,
                             mem::transmute(&val),
                             mem::size_of::<c_int>() as u32)
        };

        if res == -1 {
            return Err(nix::Error::last());
        }

        Ok(())
    }
}

use super::Error;
impl super::Ipc for Socket {
    fn recv(&self, buf: &mut [u8]) -> Result<usize, Error> {
        socket::recvmsg::<()>(self.0,
                              &[nix::sys::uio::IoVec::from_mut_slice(&mut buf[..])],
                              None,
                              nix::sys::socket::MsgFlags::empty())
            .map(|r| r.bytes)
            .map_err(|e| Error::from(e))
    }

    fn send(&self, addr: Option<u16>, buf: &[u8]) -> Result<(), Error> {
        // addr should NEVER be Some(_) for a netlink socket
        // there is no addressing for netlink.
        if addr.is_some() {
            return Err(Error(String::from("No addr for netlink")));
        }

        socket::sendmsg(self.0,
                        &[nix::sys::uio::IoVec::from_slice(&buf[..])],
                        &[],
                        nix::sys::socket::MsgFlags::empty(),
                        None)
            .map(|_| ())
            .map_err(|e| Error::from(e))
    }

    fn close(&self) -> Result<(), Error> {
        return socket::shutdown(self.0, nix::sys::socket::Shutdown::Both)
            .map_err(|e| Error::from(e));
    }
}
