use std::rc::{Rc, Weak};

use super::Error;
use super::Result;
use std::sync::{Arc, atomic};

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

/// Backend builder contains the objects
/// needed to build a new backend.
pub struct BackendBuilder<T: Ipc> {
    pub sock: T,
    pub mode: ListenMode,
}

impl<T: Ipc> BackendBuilder<T> {
    pub fn build(self, atomic_bool: Arc<atomic::AtomicBool>) -> Backend<T> {
        Backend::new(self.sock, self.mode, atomic_bool)
    }
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
/// The atomic bool is a way to stop iterating.
pub struct Backend<T: Ipc> {
    sock: Rc<T>,
    rcv_buf: Vec<u8>,
    listen_mode: ListenMode,
    continue_listening: Arc<atomic::AtomicBool>,
}

impl<T: Ipc> Backend<T> {
    /// Pass in a T: Ipc, the Ipc substrate to use.
    /// Return a Backend on which to call send_msg
    /// and listen
    pub fn new(sock: T, mode: ListenMode, atomic_bool: Arc<atomic::AtomicBool>) -> Backend<T> {
        Backend{
            sock: Rc::new(sock),
            rcv_buf: vec![0u8; 1024],
            listen_mode: mode,
            continue_listening: atomic_bool,
        }
    }

    pub fn sender(&self) -> BackendSender<T> {
        BackendSender(Rc::downgrade(&self.sock))
    }

    pub fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool> {
        Arc::clone(&(self.continue_listening))
    }
}


impl<T: Ipc> Iterator for Backend<T> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // if continue_loop has been set to false, stop iterating
            if !self.continue_listening.load(atomic::Ordering::SeqCst) {
                return None;
            }
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
