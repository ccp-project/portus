use std;
//use std::os::unix::net::UnixDatagram;
use unix_socket::{UnixDatagram};

#[cfg(target_os = "linux")]
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use unix_socket::os::linux::SocketAddrExt;

use super::Error;
use super::Result;
use std::marker::PhantomData;
use std::path::PathBuf;

pub struct Socket<T> {
    sk: UnixDatagram,
    _phantom: PhantomData<T>,
}

impl<T> Socket<T> {
    fn __new(bind_to: &str) -> Result<Self> {
        let bind_to_addr = format!("\0/ccp/{}", bind_to.to_string());
        let sock = UnixDatagram::bind(bind_to_addr)?;
        sock.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;

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
        self.sk
            .send_to(msg, to)
            .map(|_| ())
            .map_err(Error::from)
    }

    #[cfg(target_os = "linux")]
    fn recv(&self, msg: &mut [u8]) -> Result<(usize,Self::Addr)> {
        self.sk.recv_from(msg).and_then(|(size,addr)| {
            if addr.is_unnamed() {
                Err(std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, ""))
            } else {
                if let Some(path) = addr.as_pathname() {
                    Ok((size, path.to_path_buf()))
                } else if let Some(path) = addr.as_abstract() {
                    Ok((size, PathBuf::from(OsStr::from_bytes(&path))))
                } else {
                    unreachable!("named socketaddr must be path or abstract");
                }
            }
        }).or_else(|e| {
            if e.kind() != std::io::ErrorKind::WouldBlock {
                Err(Error::from(e))
            } else {
                Ok((0, PathBuf::new()))
            }
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn recv(&self, msg: &mut [u8]) -> Result<(usize,Self::Addr)> {
        self.sk.recv_from(msg).map_err(Error::from).and_then(|(size,addr)|  {

            if addr.is_unnamed() {
                Err(Error(String::from("no recv addr")))
            } else {
                if let Some(path) = addr.as_pathname() {
                    Ok((size, path.to_path_buf()))
                } else {
                    unreachable!("named socketaddr must be path (abstract does not exist on non-linux)");
                }
            }
        }
        )
    }


    fn close(&mut self) -> Result<()> {
        use std::net::Shutdown;
        self.sk.shutdown(Shutdown::Both).map_err(Error::from)
    }
}

use super::Blocking;
impl Socket<Blocking> {
    pub fn new(bind_to: &str) -> Result<Self> {
        Socket::__new(bind_to)
    }
}

use super::Nonblocking;
impl Socket<Nonblocking> {
    pub fn new(bind_to: &str) -> Result<Self> {
        let sk = Socket::__new(bind_to)?;
        sk.sk.set_nonblocking(true).map_err(Error::from)?;
        Ok(sk)
    }
}
