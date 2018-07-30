use std;
use std::os::unix::net::UnixDatagram;

use super::Error;
use super::Result;
use std::marker::PhantomData;

macro_rules! unix_addr {
    // TODO for now assumes just a single CCP (id=0)
    ($x:expr) => (format!("/tmp/ccp/0/{}", $x));
}

pub struct Socket<T> {
    sk: UnixDatagram,
    dest: String,
    _phantom: PhantomData<T>,
}

impl<T> Socket<T> {
    // Only the CCP process is allowed to use id = 0.
    // For all other datapaths, they should use a known unique identifier
    // such as the port number.
    fn __new(bind_to: &str, send_to: &str) -> Result<Self> {
        let bind_to_addr = unix_addr!(bind_to.to_string());
        let send_to_addr = unix_addr!(send_to.to_string());
        // create dir if not already exists
        match std::fs::create_dir_all("/tmp/ccp/0").err() {
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

        Ok(Socket {
            sk: sock,
            dest: send_to_addr,
            _phantom: PhantomData,
        })
    }
}

impl<T: 'static + Sync + Send> super::Ipc for Socket<T> {
    fn name() -> String {
        String::from("unix")
    }

    fn send(&self, msg: &[u8]) -> Result<()> {
        self.sk.send_to(msg, self.dest.clone())
            .map(|_| ())
            .map_err(Error::from)
    }

    fn recv(&self, msg: &mut [u8]) -> Result<usize> {
        self.sk.recv(msg).map_err(Error::from)
    }
    
    fn close(&self) -> Result<()> {
        use std::net::Shutdown;
        self.sk.shutdown(Shutdown::Both).map_err(Error::from)
    }
}

use super::Blocking;
impl Socket<Blocking> {
    pub fn new(bind_to: &str, send_to: &str) -> Result<Self> {
        Socket::__new(bind_to, send_to)
    }
}

use super::Nonblocking;
impl Socket<Nonblocking> {
    pub fn new(bind_to: &str, send_to: &str) -> Result<Self> {
        let sk = Socket::__new(bind_to, send_to)?;
        sk.sk.set_nonblocking(true).map_err(Error::from)?;
        Ok(sk)
    }
}
