use std::rc::{Rc, Weak};
use std::sync::{Arc, atomic};

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
    /// Blocking listen. Return value is how many bytes were read. Should not allocate.
    fn recv(&self, msg: &mut [u8]) -> Result<usize>;
    /// Close the underlying sockets
    fn close(&self) -> Result<()>;
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
    pub fn build<'a>(self, atomic_bool: Arc<atomic::AtomicBool>, receive_buf: &'a mut [u8]) -> Backend<'a, T> {
        Backend::new(self.sock, atomic_bool, receive_buf)
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
pub struct Backend<'a, T: Ipc> {
    sock: Rc<T>,
    continue_listening: Arc<atomic::AtomicBool>,
    receive_buf: &'a mut [u8],
    tot_read: usize,
    read_until: usize,
}

use ::serialize::Msg;
impl<'a, T: Ipc> Backend<'a, T> {
    /// Pass in a T: Ipc, the Ipc substrate to use.
    /// Return a Backend on which to call send_msg
    /// and listen
    pub fn new(
        sock: T, 
        continue_listening: Arc<atomic::AtomicBool>, 
        receive_buf: &'a mut [u8],
    ) -> Backend<'a, T> {
        Backend{
            sock: Rc::new(sock),
            continue_listening,
            receive_buf,
            tot_read: 0,
            read_until: 0,
        }
    }

    pub fn sender(&self) -> BackendSender<T> {
        BackendSender(Rc::downgrade(&self.sock))
    }

    pub fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool> {
        Arc::clone(&(self.continue_listening))
    }

    pub fn next<'b>(&'b mut self) -> Option<Msg<'b>> {
        // if we have leftover buffer from the last read, parse another message.
        if self.read_until < self.tot_read {
            let (msg, consumed) = Msg::from_buf(&self.receive_buf[self.read_until..]).ok()?;
            self.read_until += consumed;
            return Some(msg);
        } else {
            self.tot_read = self.get_next_read().ok()?;
            self.read_until = 0;
            let (msg, consumed) = Msg::from_buf(&self.receive_buf[self.read_until..self.tot_read]).ok()?;
            self.read_until += consumed;

            return Some(msg);
        }
    }
    
    /// calls ipc repeatedly to read one or more messages.
    /// Returns a slice into self.receive_buf covering the read data
    fn get_next_read<'i>(&mut self) -> Result<usize> {
        loop {
            // if continue_loop has been set to false, stop iterating
            if !self.continue_listening.load(atomic::Ordering::SeqCst) {
                return Err(Error(String::from("Done")));
            }

            let read = match self.sock.recv(self.receive_buf) {
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

impl<'a, T: Ipc> Drop for Backend<'a, T> {
    fn drop(&mut self) {
        self.sock.close().unwrap_or_else(|e| println!("{:?}", e))
    }
}

#[cfg(test)]
pub mod test;
