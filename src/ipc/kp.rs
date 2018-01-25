extern crate libc;
use std::fs::File;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::io::Read;
use std::io::Write;
use std::sync::Mutex;

use super::Error;
use super::Result;
use super::ListenMode;

pub struct Socket {
    r : Mutex<File>,
    w : Mutex<File>,
    mode: ListenMode
}

impl Socket {
    pub fn new(mode: ListenMode) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).read(true);
        match mode {
            ListenMode::Blocking => { /* do nothing */ }
            ListenMode::Nonblocking => {
                options.custom_flags(libc::O_NONBLOCK);
            }
        };

        let rfd = options.open("/dev/ccpkp")?;
        let wfd = rfd.try_clone()?;
        Ok(Socket {
            r : Mutex::new(rfd),
            w : Mutex::new(wfd),
            mode: mode
        })
    }
}

impl super::Ipc for Socket {
    fn send(&self, buf:&[u8]) -> Result<()> {
        self.w.lock().unwrap().write(buf).map_err(Error::from)?;
        Ok(())
    }

    fn recv<'a>(&self, msg:&'a mut [u8]) -> Result<&'a [u8]> {
        if let ListenMode::Nonblocking = self.mode {
            unreachable!();
        }

        let len = self.r.lock().unwrap().read(msg).map_err(Error::from)?;
        Ok(&msg[..len])
    }

    fn recv_nonblocking<'a>(&self, msg:&'a mut [u8]) -> Option<&'a [u8]> {
        if let ListenMode::Blocking = self.mode {
            unreachable!();
        }

        let len = self.r.lock().unwrap().read(msg).ok()?;
        Some(&msg[..len])
    }

    fn close(&self) -> Result<()> {
        Ok(())
    }
}

/*
#[cfg(test)]
mod tests {
    #[test]
    fn test_kp() {
        use std;
        use ipc::Ipc;

        let sk = super::Socket::new().expect("kp sock init");

        let msg = "hello, world".as_bytes();
        sk.send(&msg).expect("send msg");

        let mut msg = [0u8; 128];
        let buf = sk.recv(&mut msg).expect("recv msg");
        let got = std::str::from_utf8(buf).expect("parse string");
        assert_eq!(got, "hello, world");
    }
}
*/
