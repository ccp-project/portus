use std::marker::PhantomData;

use super::Error;
use super::Result;

extern crate libc;
use libc::c_int;
extern crate nix;
use nix::sys::socket;

pub struct Socket<T>(c_int, PhantomData<T>);

const NL_CFG_F_NONROOT_RECV: c_int = 1;
const NL_CFG_F_NONROOT_SEND: c_int = (1 << 1);
const NLMSG_HDRSIZE: usize = 0x10;

impl<T> Socket<T> {
    fn __new() -> Result<Self> {
        let fd = if let Ok(fd) = socket::socket(
            nix::sys::socket::AddressFamily::Netlink,
            nix::sys::socket::SockType::Raw,
            nix::sys::socket::SockFlag::empty(),
            libc::NETLINK_USERSOCK,
        ) {
            fd
        } else {
            socket::socket(
                nix::sys::socket::AddressFamily::Netlink,
                nix::sys::socket::SockType::Raw,
                nix::sys::socket::SockFlag::from_bits_truncate(NL_CFG_F_NONROOT_RECV)
                    | nix::sys::socket::SockFlag::from_bits_truncate(NL_CFG_F_NONROOT_SEND),
                libc::NETLINK_USERSOCK,
            )?
        };

        let pid = unsafe { libc::getpid() };

        socket::bind(fd, &nix::sys::socket::SockAddr::new_netlink(pid as u32, 0))?;

        Ok(Socket(fd, PhantomData))
    }

    pub fn new() -> Result<Self> {
        let s = Self::__new()?;
        let opt = 22;
        use std::mem;
        s.setsockopt(
            270,
            libc::NETLINK_ADD_MEMBERSHIP,
            &opt as *const i32 as *const libc::c_void,
            mem::size_of::<c_int>() as u32,
        )?;

        let to = libc::timespec {
            tv_sec: 1 as libc::time_t,
            tv_nsec: 0 as libc::c_long,
        };

        s.setsockopt(
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &to as *const libc::timespec as *const libc::c_void,
            mem::size_of::<libc::timespec>() as u32,
        )?;
        Ok(s)
    }

    fn setsockopt(
        &self,
        level: c_int,
        option: c_int,
        val: *const libc::c_void,
        sz: u32,
    ) -> Result<()> {
        let res = unsafe { libc::setsockopt(self.0, level, option as c_int, val, sz) };

        if res == -1 {
            return Err(Error::from(nix::Error::last()));
        }

        Ok(())
    }

    fn __recv(&self, buf: &mut [u8], flags: nix::sys::socket::MsgFlags) -> Result<usize> {
        let mut nl_buf = [0u8; 1024];
        let end = socket::recvmsg::<()>(
            self.0,
            &[nix::sys::uio::IoVec::from_mut_slice(&mut nl_buf[..])],
            None,
            flags,
        )
        .map(|r| r.bytes)
        .map_err(Error::from)?;
        buf[..(end - NLMSG_HDRSIZE)].copy_from_slice(&nl_buf[NLMSG_HDRSIZE..end]);
        Ok(end - NLMSG_HDRSIZE)
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
    fn __send(&self, buf: &[u8]) -> Result<()> {
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
        )
        .map(|_| ())
        .map_err(Error::from)
    }

    fn __close(&mut self) -> Result<()> {
        let ok = unsafe { libc::close(self.0) as i32 };
        if ok < 0 {
            Err(Error(format!("could not close netlink socket: {}", ok)))
        } else {
            Ok(())
        }
    }
}

use super::Blocking;
impl super::Ipc for Socket<Blocking> {
    type Addr = ();

    fn name() -> String {
        String::from("netlink")
    }

    fn recv(&self, buf: &mut [u8]) -> Result<(usize, Self::Addr)> {
        self.__recv(buf, nix::sys::socket::MsgFlags::empty()).map(|s| (s,()))
    }

    fn send(&self, buf: &[u8], _to: &Self::Addr) -> Result<()> {
        self.__send(buf)
    }

    fn close(&mut self) -> Result<()> {
        self.__close()
    }
}

use super::Nonblocking;
impl super::Ipc for Socket<Nonblocking> {
    type Addr = ();

    fn name() -> String {
        String::from("netlink")
    }

    fn recv(&self, buf: &mut [u8]) -> Result<(usize, Self::Addr)> {
        self.__recv(buf, nix::sys::socket::MSG_DONTWAIT).map(|s| (s,()))
    }

    fn send(&self, buf: &[u8], _to: &Self::Addr) -> Result<()> {
        self.__send(buf)
    }

    fn close(&mut self) -> Result<()> {
        self.__close()
    }
}
