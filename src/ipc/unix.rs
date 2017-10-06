use std;
use std::os::unix::net::UnixDatagram;

macro_rules! port_to_addr {
    ($x:expr) => (format!("/tmp/ccp/{}", $x));
}

macro_rules! translate_result {
    ($x:expr) => ($x.map(|_| ()).map_err(|e| super::Error::from(e)));
}

pub struct Socket {
    sk: UnixDatagram,
    is_connected: bool,
}

impl Socket {
    // Only the CCP process is allowed to use id = 0.
    // For all other datapaths, they should use a known unique identifier
    // such as the port number.
    pub fn new(port: u16) -> Result<Self, super::Error> {
        // create dir if not already exists
        match std::fs::create_dir("/tmp/ccp").err() {
            Some(ref e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Some(e) => Err(e),
            None => Ok(()),
        }?;

        let in_addr = port_to_addr!(port);

        // unlink before bind
        match std::fs::remove_file(&in_addr).err() {
            Some(ref e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Some(e) => Err(e),
            None => Ok(()),
        }?;
        let sock = UnixDatagram::bind(in_addr)?;

        if port != 0 {
            sock.connect(port_to_addr!(0))?;
            Ok(Socket {
                sk: sock,
                is_connected: true,
            })
        } else {
            Ok(Socket {
                sk: sock,
                is_connected: false,
            })
        }
    }
}

impl super::Ipc for Socket {
    fn send(&self, addr: Option<u16>, msg: &[u8]) -> Result<(), super::Error> {
        match self {
            &Socket {
                ref sk,
                is_connected: true,
            } => {
                if addr.is_some() {
                    Err(super::Error(
                        String::from("No addr for connected unix socket"),
                    ))
                } else {
                    translate_result!(sk.send(msg))
                }
            }

            &Socket {
                ref sk,
                is_connected: false,
            } => {
                match addr {
                    Some(a) => translate_result!(sk.send_to(msg, port_to_addr!(a))),
                    None => Err(super::Error(
                        String::from("Need addr for unconnected unix socket"),
                    )),
                }
            }
        }
    }

    // return the number of bytes read if successful.
    fn recv(&self, msg: &mut [u8]) -> Result<usize, super::Error> {
        self.sk.recv(msg).map_err(|e| super::Error::from(e))
    }

    fn close(&self) -> Result<(), super::Error> {
        use std::net::Shutdown;
        translate_result!(self.sk.shutdown(Shutdown::Both))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(linux))] // this doesn't work on Darwin currently. Not sure why.
    #[test]
    fn test_sk() {
        use std;
        use ipc::Ipc;

        let sk1 = super::Socket::new(0).expect("recv socket init");
        assert!(!sk1.is_connected);

        let sk2 = super::Socket::new(42424).expect("send socket init");
        assert!(sk2.is_connected);

        use std::thread;

        let c2 = thread::spawn(move || {
            let msg = "hello, world".as_bytes();
            sk2.send(None, &msg).expect("send msg");
            sk2.close().expect("close sender");
        });

        let mut msg = [0u8; 128];
        let sz = sk1.recv(&mut msg).expect("receive msg");
        sk1.close().expect("close receiver");
        let got = std::str::from_utf8(&msg[..sz]).expect("parse string");
        assert_eq!(got, "hello, world");
        c2.join().expect("join sender thread");
    }
}
