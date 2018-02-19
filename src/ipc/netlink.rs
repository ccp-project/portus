use super::Error;
use super::Result;

extern crate libc;
use libc::c_int;
extern crate nix;
use nix::sys::socket;

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Self {
        Error(format!("err {}", e))
    }
}

#[derive(Debug)]
pub struct Socket(c_int);

const NL_CFG_F_NONROOT_RECV: c_int = 1;
const NL_CFG_F_NONROOT_SEND: c_int = (1 << 1);

impl Socket {
    fn __new() -> Result<Self> {
        let fd = if let Ok(fd) = socket::socket(
            nix::sys::socket::AddressFamily::Netlink,
            nix::sys::socket::SockType::Raw,
            nix::sys::socket::SockFlag::empty(),
            libc::NETLINK_USERSOCK,
        )
        {
            fd
        } else {
            socket::socket(
                nix::sys::socket::AddressFamily::Netlink,
                nix::sys::socket::SockType::Raw,
                nix::sys::socket::SockFlag::from_bits_truncate(NL_CFG_F_NONROOT_RECV) |
                    nix::sys::socket::SockFlag::from_bits_truncate(NL_CFG_F_NONROOT_SEND),
                libc::NETLINK_USERSOCK,
            )?
        };

        let pid = unsafe { libc::getpid() };

        socket::bind(fd, &nix::sys::socket::SockAddr::new_netlink(pid as u32, 0))?;

        Ok(Socket(fd))
    }

    pub fn new() -> Result<Self> {
        let s = Self::__new()?;
        s.setsockopt_int(270, libc::NETLINK_ADD_MEMBERSHIP, 22)?;
        Ok(s)
    }

    fn setsockopt_int(&self, level: c_int, option: c_int, val: c_int) -> Result<()> {
        use std::mem;
        let res = unsafe {
            libc::setsockopt(
                self.0,
                level,
                option as c_int,
                &val as *const i32 as *const libc::c_void,
                mem::size_of::<c_int>() as u32,
            )
        };

        if res == -1 {
            return Err(Error::from(nix::Error::last()));
        }

        Ok(())
    }
}

const NLMSG_HDRSIZE: usize = 0x10;
impl super::Ipc for Socket {
    fn recv<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8]> {
        let res = socket::recvmsg::<()>(
            self.0,
            &[nix::sys::uio::IoVec::from_mut_slice(&mut buf[..])],
            None,
            nix::sys::socket::MsgFlags::empty(),
        ).map(|r| r.bytes)
            .map_err(Error::from)?;
        Ok(&buf[NLMSG_HDRSIZE..res])
    }

    // netlink header format (RFC 3549)
    // 0               1               2               3
    // 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7 0 1 2 3 4 5 6 7
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                          Length                             |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |            Type              |           Flags              |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                      Sequence Number                        |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    // |                      Process ID (PID)                       |
    // +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
    fn send(&self, buf: &[u8]) -> Result<()> {
        let len = NLMSG_HDRSIZE + buf.len();
        let mut msg = Vec::<u8>::with_capacity(len);
        msg.resize(4, 0u8);
        // write the netlink header
        super::super::serialize::u32_to_u8s(&mut msg[0..4], len as u32);
        // rest is 0s
        msg.extend_from_slice(&[0u8; 12]);
        // payload
        msg.extend_from_slice(buf);

        // send
        socket::sendmsg(
            self.0,
            &[nix::sys::uio::IoVec::from_slice(&msg[..])],
            &[],
            nix::sys::socket::MsgFlags::empty(),
            None,
        ).map(|_| ())
            .map_err(Error::from)
    }

    fn close(&self) -> Result<()> {
        socket::shutdown(self.0, nix::sys::socket::Shutdown::Both)
            .map_err(Error::from)
    }
}
