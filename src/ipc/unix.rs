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
    dest : String

}

impl Socket {
    // Only the CCP process is allowed to use id = 0.
    // For all other datapaths, they should use a known unique identifier
    // such as the port number.
    pub fn new(bind_to : &str, send_to : &str) -> Result<Self> {
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
    fn recv<'a>(&self, msg: &'a mut [u8]) -> Result<&'a [u8]> {
        let sz = self.sk.recv(msg).map_err(Error::from)?;
        Ok(&msg[..sz])
    }

    fn close(&self) -> Result<()> {
        use std::net::Shutdown;
        translate_result!(self.sk.shutdown(Shutdown::Both))
    }
}

#[cfg(test)]
mod tests {
    // TODO : this doesn't work on Darwin currently. Not sure why.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_sk() {
        use std;
        use ipc::Ipc;

        let sk1 = super::Socket::new("out", "in").expect("recv socket init");
        let sk2 = super::Socket::new("in", "out").expect("send socket init");

        use std::{time,thread};

        let c2 = thread::spawn(move || {
            let msg = "hello, world".as_bytes();
            sk2.send(&msg).expect("send msg");
            sk2.close().expect("close sender");
        });

        let mut msg = [0u8; 128];
        // TODO : no idea why this sleep is necessary, fixes for now
        thread::sleep(time::Duration::from_millis(1));
        let buf = sk1.recv(&mut msg).expect("receive msg");
        sk1.close().expect("close receiver");
        let got = std::str::from_utf8(buf).expect("parse string");
        assert_eq!(got, "hello, world");
        c2.join().expect("join sender thread");
    }
}
