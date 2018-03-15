use std::rc::{Rc, Weak};

use super::Error;
use super::Result;

#[cfg(all(target_os = "linux"))]
pub mod netlink;
pub mod unix;
#[cfg(all(target_os = "linux"))]
pub mod kp;

pub trait Ipc: 'static + Sync + Send {
    /// Blocking send
    fn send(&self, msg: &[u8]) -> Result<()>;
    /// Blocking listen. Return value is a slice into the provided buffer. Should not allocate.
    fn recv<'a>(&self, msg: &'a mut [u8]) -> Result<&'a [u8]>;
    /// Non-blocking listen. Return value is a slice into the provided buffer. Should not allocate.
    fn recv_nonblocking<'a>(&self, msg: &'a mut [u8]) -> Option<&'a [u8]>;
    /// Close the underlying sockets
    fn close(&self) -> Result<()>;
}

#[derive(Copy, Clone)]
pub enum ListenMode {
    Blocking,
    Nonblocking,
}

pub struct BackendSender<T: Ipc>(Weak<T>);

impl<T: Ipc> BackendSender<T> {
    /// Blocking send.
    pub fn send_msg(&self, msg: &[u8]) -> Result<()> {
        let s = Weak::upgrade(&self.0).ok_or_else(|| Error(String::from("Send on closed IPC socket!")))?;
        s.send(msg).map_err(Error::from)
    }
}

impl<T: Ipc> Clone for BackendSender<T> {
    fn clone(&self) -> Self {
        BackendSender(self.0.clone())
    }
}

/// Backend will yield incoming IPC messages forever.
/// It owns the socket; senders hold weak references.
pub struct Backend<T: Ipc> {
    sock: Rc<T>,
    rcv_buf: Vec<u8>,
    listen_mode: ListenMode,
}

impl<T: Ipc> Backend<T> {
    /// Pass in a T: Ipc, the Ipc substrate to use.
    /// Return a Backend on which to call send_msg
    /// and listen
    pub fn new(sock: T, mode: ListenMode) -> Backend<T> {
        Backend{
            sock: Rc::new(sock),
            rcv_buf: vec![0u8; 1024],
            listen_mode: mode,
        }
    }

    pub fn sender(&self) -> BackendSender<T> {
        BackendSender(Rc::downgrade(&self.sock))
    }
}

impl<T: Ipc> Iterator for Backend<T> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let buf = match self.listen_mode {
                ListenMode::Blocking => 
                    match self.sock.recv(&mut self.rcv_buf) {
                        Ok(l) => l,
                        Err(_) => continue,
                    },
                ListenMode::Nonblocking => 
                    match self.sock.recv_nonblocking(&mut self.rcv_buf) {
                        Some(l) => l,
                        None => continue,
                    },
            };

            if buf.is_empty() {
                continue;
            }

            return Some(buf.to_vec());
        }
    }
}

impl<T: Ipc> Drop for Backend<T> {
    fn drop(&mut self) {
        self.sock.close().unwrap_or_else(|e| println!("{:?}", e))
    }
}

#[cfg(test)]
pub mod test;
