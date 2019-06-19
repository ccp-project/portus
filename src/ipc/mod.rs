//! A library wrapping various IPC mechanisms with a datagram-oriented
//! messaging layer. This is how CCP communicates with the datapath.

use std::rc::{Rc, Weak};
use std::sync::{atomic, Arc};

use super::Error;
use super::Result;

/// Thread-channel implementation
pub mod chan;
#[cfg(all(target_os = "linux"))]
/// Character device implementation
pub mod kp;
#[cfg(all(target_os = "linux"))]
/// Netlink socket implementation
pub mod netlink;
/// Unix domain socket implementation
pub mod unix;

/// IPC mechanisms must implement this trait.
pub trait Ipc: 'static + Send {
    /// Returns the name of this IPC mechanism (e.g. "netlink" for Linux netlink sockets)
    fn name() -> String;
    /// Blocking send
    fn send(&self, msg: &[u8]) -> Result<()>;
    /// Blocking listen. Return value is how many bytes were read. Should not allocate.
    fn recv(&self, msg: &mut [u8]) -> Result<usize>;
    /// Close the underlying sockets
    fn close(&mut self) -> Result<()>;
}

/// Marker type specifying that the IPC socket should make blocking calls to the underlying socket
pub struct Blocking;
/// Marker type specifying that the IPC socket should make nonblocking calls to the underlying socket
pub struct Nonblocking;

/// Backend builder contains the objects
/// needed to build a new backend.
pub struct BackendBuilder<T: Ipc> {
    pub sock: T,
}

impl<T: Ipc> BackendBuilder<T> {
    pub fn build(
        self,
        atomic_bool: Arc<atomic::AtomicBool>,
    ) -> Backend<T> {
        Backend::new(self.sock, atomic_bool)
    }
}

/// A send-only handle to the underlying IPC socket.
pub struct BackendSender<T: Ipc>(Weak<T>);

impl<T: Ipc> BackendSender<T> {
    /// Blocking send.
    pub fn send_msg(&self, msg: &[u8]) -> Result<()> {
        let s = Weak::upgrade(&self.0)
            .ok_or_else(|| Error(String::from("Send on closed IPC socket!")))?;
        s.send(msg).map_err(Error::from)
    }
}

impl<T: Ipc> Clone for BackendSender<T> {
    fn clone(&self) -> Self {
        BackendSender(self.0.clone())
    }
}

/// Backend will yield incoming IPC messages forever via `next()`.
/// It owns the socket; `BackendSender` holds weak references.
/// The atomic bool is a way to stop iterating.
pub struct Backend<T: Ipc> {
    sock: Rc<T>,
    continue_listening: Arc<atomic::AtomicBool>,
    receive_buf: [u8; 1024],
    tot_read: usize,
    read_until: usize,
}

use crate::serialize::Msg;
impl<T: Ipc> Backend<T> {
    pub fn new(
        sock: T,
        continue_listening: Arc<atomic::AtomicBool>,
    ) -> Backend<T> {
        Backend {
            sock: Rc::new(sock),
            continue_listening,
            receive_buf: [0u8; 1024],
            tot_read: 0,
            read_until: 0,
        }
    }

    pub fn sender(&self) -> BackendSender<T> {
        BackendSender(Rc::downgrade(&self.sock))
    }

    /// Return a copy of the flag variable that indicates that the
    /// `Backend` should continue listening (i.e., not exit).
    pub fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool> {
        Arc::clone(&(self.continue_listening))
    }

    /// Get the next IPC message.
    // This is similar to `impl Iterator`, but the returned value is tied to the lifetime
    // of `self`, so we cannot implement that trait.
    pub fn next(&mut self) -> Option<Msg<'_>> {
        // if we have leftover buffer from the last read, parse another message.
        if self.read_until < self.tot_read {
            let (msg, consumed) = Msg::from_buf(&self.receive_buf[self.read_until..]).ok()?;
            self.read_until += consumed;
            Some(msg)
        } else {
            self.tot_read = self.get_next_read().ok()?;
            self.read_until = 0;
            let (msg, consumed) =
                Msg::from_buf(&self.receive_buf[self.read_until..self.tot_read]).ok()?;
            self.read_until += consumed;

            Some(msg)
        }
    }

    // calls IPC repeatedly to read one or more messages.
    // Returns a slice into self.receive_buf covering the read data
    fn get_next_read(&mut self) -> Result<usize> {
        loop {
            // if continue_loop has been set to false, stop iterating
            if !self.continue_listening.load(atomic::Ordering::SeqCst) {
                return Err(Error(String::from("Done")));
            }

            let read = match self.sock.recv(&mut self.receive_buf) {
                Ok(l) => l,
                _ => continue,
            };

            if read == 0 {
                continue;
            }

            return Ok(read);
        }
    }
}

impl<T: Ipc> Drop for Backend<T> {
    fn drop(&mut self) {
        Rc::get_mut(&mut self.sock)
            .ok_or_else(|| {
                Error(String::from(
                    "Could not get exclusive ref to socket to close",
                ))
            })
            .and_then(Ipc::close)
            .unwrap_or_else(|_| ());
    }
}

#[cfg(test)]
pub mod test;
