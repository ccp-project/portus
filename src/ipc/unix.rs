#[cfg(target_os = "linux")]
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use tracing::trace;
use unix_socket::os::linux::SocketAddrExt;
use unix_socket::UnixDatagram;

use super::Error;
use super::Result;
use std::marker::PhantomData;

pub struct Socket<T> {
    sk: UnixDatagram,
    _phantom: PhantomData<T>,
}

impl<T> Socket<T> {
    fn __new(
        bind_to: &str,
        sndbuf_bytes: Option<usize>,
        rcvbuf_bytes: Option<usize>,
    ) -> Result<Self> {
        let bind_to_addr = format!("\0/ccp/{}", bind_to.to_string());
        let sock = UnixDatagram::bind(bind_to_addr)?;
        sock.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
        if let Some(sb) = sndbuf_bytes {
            let snd_res = nix::sys::socket::setsockopt(
                sock.as_raw_fd(),
                nix::sys::socket::sockopt::SndBuf,
                &sb,
            );
            trace!(?sndbuf_bytes, is_ok=?snd_res.is_ok(), "set send buf sockopt");
        }

        if let Some(rb) = rcvbuf_bytes {
            let rcv_res = nix::sys::socket::setsockopt(
                sock.as_raw_fd(),
                nix::sys::socket::sockopt::RcvBuf,
                &rb,
            );
            trace!(?rcvbuf_bytes, is_ok=?rcv_res.is_ok(), "set rcv buf sockopt");
        }

        Ok(Socket {
            sk: sock,
            _phantom: PhantomData,
        })
    }
}

impl<T: 'static + Sync + Send> super::Ipc for Socket<T> {
    type Addr = OsString;

    fn name() -> String {
        String::from("unix")
    }

    fn send(&self, msg: &[u8], to: &Self::Addr) -> Result<()> {
        self.sk.send_to(msg, to).map(|_| ()).map_err(Error::from)
    }

    #[cfg(target_os = "linux")]
    fn recv(&self, msg: &mut [u8]) -> Result<(usize, Self::Addr)> {
        let res = loop {
            match self.sk.recv_from(msg).and_then(|(size, addr)| {
                if addr.is_unnamed() {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::AddrNotAvailable,
                        "",
                    ))
                } else {
                    if let Some(path) = addr.as_pathname() {
                        Ok((size, path.to_path_buf().into_os_string()))
                    } else if let Some(path) = addr.as_abstract() {
                        let mut real_path = OsString::with_capacity(path.len() + 1);
                        real_path.push("\0");
                        real_path.push(OsStr::from_bytes(&path));
                        Ok((size, real_path))
                    } else {
                        unreachable!("named socketaddr must be path or abstract");
                    }
                }
            }) {
                Ok(r) => break Ok(r),
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::Interrupted {
                        trace!("got EINTR, ignoring...");
                        continue;
                    } else if e.kind() != std::io::ErrorKind::WouldBlock {
                        break Err(Error::from(e));
                    } else {
                        break Ok((0, OsString::new()));
                    }
                }
            }
        };
        res
    }

    #[cfg(not(target_os = "linux"))]
    fn recv(&self, msg: &mut [u8]) -> Result<(usize, Self::Addr)> {
        self.sk
            .recv_from(msg)
            .map_err(Error::from)
            .and_then(|(size, addr)| {
                if addr.is_unnamed() {
                    Err(Error(String::from("no recv addr")))
                } else {
                    if let Some(path) = addr.as_pathname() {
                        Ok((size, path.to_path_buf().into_os_string()))
                    } else {
                        unreachable!(
                            "named socketaddr must be path (abstract does not exist on non-linux)"
                        );
                    }
                }
            })
    }

    fn close(&mut self) -> Result<()> {
        use std::net::Shutdown;
        self.sk.shutdown(Shutdown::Both).map_err(Error::from)
    }
}

use super::Blocking;
impl Socket<Blocking> {
    pub fn new(bind_to: &str) -> Result<Self> {
        Socket::__new(bind_to, None, None)
    }

    pub fn new_with_skbuf(
        bind_to: &str,
        sndbuf_bytes: Option<usize>,
        rcvbuf_bytes: Option<usize>,
    ) -> Result<Self> {
        Socket::__new(bind_to, sndbuf_bytes, rcvbuf_bytes)
    }
}

use super::Nonblocking;
impl Socket<Nonblocking> {
    pub fn new(bind_to: &str) -> Result<Self> {
        Socket::__new(bind_to, None, None)
    }

    pub fn new_with_skbuf(
        bind_to: &str,
        sndbuf_bytes: Option<usize>,
        rcvbuf_bytes: Option<usize>,
    ) -> Result<Self> {
        let sk = Socket::__new(bind_to, sndbuf_bytes, rcvbuf_bytes)?;
        sk.sk.set_nonblocking(true).map_err(Error::from)?;
        Ok(sk)
    }
}
