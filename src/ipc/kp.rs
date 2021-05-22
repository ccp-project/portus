use std::fs::File;
use std::fs::OpenOptions;
use std::marker::PhantomData;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

use super::Error;
use super::Result;

pub struct Socket<T> {
    fd: File,
    _phantom: PhantomData<T>,
}

impl<T> Socket<T> {
    fn mk_opts() -> std::fs::OpenOptions {
        let mut options = OpenOptions::new();
        options.write(true).read(true);
        options
    }

    fn open(options: std::fs::OpenOptions) -> Result<Self> {
        let file = options.open("/dev/ccpkp")?;
        Ok(Socket {
            fd: file,
            _phantom: PhantomData,
        })
    }
}

impl<T: 'static + Sync + Send> super::Ipc for Socket<T> {
    type Addr = ();

    fn name() -> String {
        String::from("char")
    }

    fn send(&self, buf: &[u8], _to: &Self::Addr) -> Result<()> {
        nix::unistd::write(self.fd.as_raw_fd(), buf)
            .map(|_| ())
            .map_err(Error::from)
    }

    fn recv(&self, msg: &mut [u8]) -> Result<(usize, Self::Addr)> {
        let pollfd = nix::poll::PollFd::new(self.fd.as_raw_fd(), nix::poll::POLLIN);
        let ok = nix::poll::poll(&mut [pollfd], 1000)?;
        if ok < 0 {
            return Err(Error::from(std::io::Error::from_raw_os_error(ok)));
        }

        let len = nix::unistd::read(self.fd.as_raw_fd(), msg).map_err(Error::from)?;
        Ok((len, ()))
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

use super::Blocking;
impl Socket<Blocking> {
    pub fn new() -> Result<Self> {
        Self::open(Self::mk_opts())
    }
}

use super::Nonblocking;
impl Socket<Nonblocking> {
    pub fn new() -> Result<Self> {
        let mut options = Self::mk_opts();
        options.custom_flags(libc::O_NONBLOCK);
        Self::open(options)
    }
}
