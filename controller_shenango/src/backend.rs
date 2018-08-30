#[cfg(feature = "iokernel")]
use shenango;
use std;
use libc;

// libc = ? 
use std::any::Any;
use std::io;
use std::net::{SocketAddr, SocketAddrV4, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

#[derive(Copy, Clone)]
pub enum Backend {
    Linux,
    #[cfg(feature = "iokernel")]
    Shenango,
}

impl Backend {
    pub fn listen_udp(
        &self,
        local_addr: SocketAddrV4,
    ) -> UdpConnection {
        match self {
            &Backend::Linux =>
                UdpConnection::Linux(UdpSocket::bind(local_addr).unwrap()),
            #[cfg(feature = "iokernel")]
            &Backend::Shenango =>
                UdpConnection::Shenango(shenango::udp::UdpConnection::listen(local_addr)),
        }
    }

    pub fn spawn_thread<T, F>(&self, f: F) -> JoinHandle<T>
    where
        T: Send,
        F: FnOnce() -> T,
        F: Send + 'static,
    {
        match *self {
            Backend::Linux => JoinHandle::Linux(thread::spawn(f)),
            #[cfg(feature = "iokernel")]
            Backend::Shenango => JoinHandle::Shenango(shenango::thread::spawn(f)),
        }
    }

    pub fn sleep(&self, duration: Duration) {
        match *self {
            Backend::Linux => thread::sleep(duration),
            #[cfg(feature = "iokernel")]
            Backend::Shenango => shenango::sleep(duration),
        }
    }

    #[allow(unused)]
    pub fn thread_yield(&self) {
        match *self {
            Backend::Linux => thread::yield_now(),
            #[cfg(feature = "iokernel")]
            Backend::Shenango => shenango::thread_yield(),
        }
    }

    pub fn init_and_run<'a, F>(&self, cfgpath: Option<&'a str>, f: F)
    where
        F: FnOnce(),
        F: Send + 'static,
    {
        match *self {
            Backend::Linux => f(),
            #[cfg(feature = "iokernel")]
            Backend::Shenango => shenango::runtime_init(cfgpath.unwrap().to_owned(), f).unwrap(),
        }
    }
}

pub enum UdpConnection {
    Linux(UdpSocket),
    #[cfg(feature = "iokernel")]
    Shenango(shenango::udp::UdpConnection),
}
impl UdpConnection {
    pub fn send_to(&self, buf: &[u8], addr: SocketAddrV4) -> io::Result<usize> {
        match *self {
            UdpConnection::Linux(ref s) => s.send_to(buf, addr),
            #[cfg(feature = "iokernel")]
            UdpConnection::Shenango(ref s) => s.write_to(buf, addr),
        }
    }
    pub fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddrV4)> {
        match *self {
            UdpConnection::Linux(ref s) => s.recv_from(buf).map(|(len, addr)| match addr {
                SocketAddr::V4(addr) => (len, addr),
                _ => unreachable!(),
            }),
            #[cfg(feature = "iokernel")]
            UdpConnection::Shenango(ref s) => s.read_from(buf),
        }
    }

    pub fn send(&self, buf: &[u8]) -> io::Result<usize> {
        match *self {
            UdpConnection::Linux(ref s) => s.send(buf),
            #[cfg(feature = "iokernel")]
            UdpConnection::Shenango(ref s) => s.send(buf),
        }
    }
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        match *self {
            UdpConnection::Linux(ref s) => s.recv(buf),
            #[cfg(feature = "iokernel")]
            UdpConnection::Shenango(ref s) => s.recv(buf),
        }
    }

    pub fn local_addr(&self) -> SocketAddrV4 {
        match *self {
            UdpConnection::Linux(ref s) => match s.local_addr() {
                Ok(SocketAddr::V4(addr)) => addr,
                _ => unreachable!(),
            },
            #[cfg(feature = "iokernel")]
            UdpConnection::Shenango(ref s) => s.local_addr(),
        }
    }

    #[allow(unused)]
    pub fn shutdown(&self) {
        match *self {
            UdpConnection::Linux(ref s) => unsafe {
                let _ = libc::shutdown(s.as_raw_fd(), libc::SHUT_RD);
            },
            #[cfg(feature = "iokernel")]
            UdpConnection::Shenango(ref s) => s.shutdown(),
        }
    }
}

pub enum JoinHandle<T: Send + 'static> {
    Linux(std::thread::JoinHandle<T>),
    #[cfg(feature = "iokernel")]
    Shenango(shenango::thread::JoinHandle<T>),
}
impl<T: Send + 'static> JoinHandle<T> {
    pub fn join(self) -> Result<T, Box<Any + Send + 'static>> {
        match self {
            JoinHandle::Linux(j) => j.join(),
            #[cfg(feature = "iokernel")]
            JoinHandle::Shenango(j) => j.join(),
        }
    }
}
