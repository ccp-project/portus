extern crate libc;
extern crate nix;

use std;
use std::fs::OpenOptions;
use std::fs::File;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

use super::Error;
use super::Result;
use super::ListenMode;

pub struct Socket {
    fd : File,
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

        let file = options.open("/dev/ccpkp")?;
        Ok(Socket {
            fd : file,
            mode,
        })
    }

    pub fn __recv(&self, msg:&mut [u8]) -> Result<usize> {
        let pollfd = nix::poll::PollFd::new(self.fd.as_raw_fd(), nix::poll::POLLIN);
        let ok = nix::poll::poll(&mut [pollfd], 1000)?;
        if ok < 0 {
            return Err(Error::from(std::io::Error::from_raw_os_error(ok)));
        }

        let len = nix::unistd::read(self.fd.as_raw_fd(), msg).map_err(Error::from)?;
        Ok(len)
    }
}

impl super::Ipc for Socket {
    fn send(&self, buf:&[u8]) -> Result<()> {
        nix::unistd::write(self.fd.as_raw_fd(), buf)
            .map(|_| ())
            .map_err(Error::from)
    }

    fn recv(&self, msg: &mut [u8]) -> Result<usize> {
        if let ListenMode::Nonblocking = self.mode {
            panic!("Blocking call on nonblocking file");
        }

        self.__recv(msg)
    }

    fn recv_nonblocking(&self, msg:&mut [u8]) -> Option<usize> {
        if let ListenMode::Blocking = self.mode {
            panic!("Nonblocking call on blocking file");
        }

        self.__recv(msg).ok()
    }

    fn close(&self) -> Result<()> {
        Ok(())
    }
}
