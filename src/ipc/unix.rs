use super::{Error, Result};
use std::marker::PhantomData;
use std::os::unix::{io::AsRawFd, net::UnixDatagram};
use std::path::PathBuf;
use tracing::trace;

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
        let bind_to_addr = format!("/tmp/ccp/{}", bind_to.to_string());
        // create dir if not already exists
        match std::fs::create_dir_all("/tmp/ccp/").err() {
            Some(ref e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Some(e) => Err(e),
            None => Ok(()),
        }?;

        // unlink before bind
        match std::fs::remove_file(&bind_to_addr).err() {
            Some(ref e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Some(e) => Err(e),
            None => Ok(()),
        }?;

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
    type Addr = PathBuf;

    fn name() -> String {
        String::from("unix")
    }

    fn send(&self, msg: &[u8], to: &Self::Addr) -> Result<()> {
        let to = format!(
            "/tmp/ccp/{}",
            to.as_path()
                .as_os_str()
                .to_str()
                .ok_or_else(|| Error("invalid addrress".to_owned()))?
        );
        self.sk.send_to(msg, to).map(|_| ()).map_err(Error::from)
    }

    fn recv(&self, msg: &mut [u8]) -> Result<(usize, Self::Addr)> {
        self.sk
            .recv_from(msg)
            .map_err(Error::from)
            .and_then(|(size, addr)| match addr.as_pathname() {
                Some(p) => Ok((size, p.to_path_buf())),
                None => Err(Error(String::from("no recv addr"))),
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
