use std;
use std::os::unix::net::UnixDatagram;

use super::Error;
use super::Result;

macro_rules! unix_addr {
    // TODO for now assumes just a single CCP (id=0)
    ($x:expr) => (format!("/tmp/ccp/0/{}", $x));
}

macro_rules! translate_result {
    ($x:expr) => ($x.map(|_| ()).map_err(super::Error::from));
}

pub struct Socket {
    sk: UnixDatagram,
    dest : String,
}

impl Socket {
    // Only the CCP process is allowed to use id = 0.
    // For all other datapaths, they should use a known unique identifier
    // such as the port number.
    pub fn new(bind_to: &str, send_to: &str) -> Result<Self> {
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
            dest: send_to_addr
        })
    }
}

impl super::Ipc for Socket {
    fn send(&self, msg: &[u8]) -> Result<()> {
        translate_result!(self.sk.send_to(msg, self.dest.clone()))
    }

    // return the number of bytes read if successful.
    fn recv(&self, msg: &mut [u8]) -> Result<usize> {
        let sz = self.sk.recv(msg).map_err(Error::from)?;
        Ok(sz)
    }
    
    fn recv_nonblocking(&self, msg: &mut [u8]) -> Option<usize> {
        self.sk.set_nonblocking(true).ok()?;
        let sz = self.sk.recv(msg).ok()?;
        Some(sz)
    }

    fn close(&self) -> Result<()> {
        use std::net::Shutdown;
        translate_result!(self.sk.shutdown(Shutdown::Both))
    }
}
