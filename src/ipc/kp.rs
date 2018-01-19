use std::fs::File;
use std::fs::OpenOptions;

use super::Error;
use super::Result;

use std::io::Read;
use std::io::Write;

use std::sync::Mutex;
//use std::sync::arc::UnsafeArc;

pub struct Socket {
    r : Mutex<File>,
    w : Mutex<File>
}


impl Socket {

    pub fn new() -> Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).read(true);
        let rfd = options.open("/dev/ccpkp")?;
        let wfd = rfd.try_clone()?;


        //let fd = File::open("/dev/ccpkp")?;
        Ok(Socket {
            r : Mutex::new(rfd),
            w : Mutex::new(wfd)
        })
    }
}

impl super::Ipc for Socket {
    fn send(&self, buf:&[u8]) -> Result<()> {
        //let len = 
        self.w.lock().unwrap().write(buf).map_err(|e| Error::from(e))?;
        //if len <= 0 {
        //    Err(super::Error(String::from("Write failed"),))
        //}
        Ok(())
    }

    fn recv<'a>(&self, msg:&'a mut [u8]) -> Result<&'a [u8]> {
        let len = self.r.lock().unwrap().read(msg).map_err(|e| Error::from(e))?;
        //if len <= 0 {
        //    Err(super::Error(String::from("Read failed"),))
        //}
        Ok(&msg[..len])
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
