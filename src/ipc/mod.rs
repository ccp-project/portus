//! A library wrapping various IPC mechanisms with a datagram-oriented
//! messaging layer. This is how CCP communicates with the datapath.

//use std::rc::{Rc, Weak};
use std::sync::{atomic, Arc, Weak};

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

/// This trait allows for the backend to be either single or mutli-threaded
pub trait Backend<T: Ipc> {
    //fn new(sock: T, continue_listening: Arc<atomic::AtomicBool>) -> Self;
    fn sender(&self) -> BackendSender<T>;
    fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool>;
    fn next(&mut self) -> Option<Msg>;
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
    ) -> SingleBackend<T> {
        SingleBackend::new(self.sock, atomic_bool)
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

/*
pub struct MultiBackendBuilder<T: Ipc> {
    pub socks: Vec<T>,
}
impl<T: Ipc> MultiBackendBuilder<T> {
    pub fn build(
        self,
        atomic_bool: Arc<atomic::AtomicBool>,
    ) -> MultiBackend<T> {
        MultiBackend::new(self.socks, atomic_bool)
    }
}
*/

use crossbeam::channel::{Receiver, Select, unbounded};
pub struct MultiBackend<T: Ipc> {
    last_recvd: Option<usize>,
    continue_listening: Arc<atomic::AtomicBool>,
    sel: Select<'static>,
    backends: Vec<BackendSender<T>>,
    receivers: &'static [Receiver<Option<Msg>>],
    receivers_ptr: *mut [Receiver<Option<Msg>>],
}

impl<T> MultiBackend<T> where T: Ipc + std::marker::Sync {
    fn new(
        socks: Vec<T>,
        continue_listening: Arc<atomic::AtomicBool>,
    ) -> Self {

        let mut backends = Vec::new();
        let mut receivers = Vec::new();

        for sock in socks {
            let mut backend = SingleBackend::new(sock, Arc::clone(&continue_listening));
            backends.push(backend.sender());

            let (s,r) = unbounded();
            receivers.push(r);
            //sel.recv(&receivers[receivers.len()-1]);

            std::thread::spawn(move || {
                loop {
                    s.send(backend.next()).unwrap() // TODO don't unwrap
                }
            });
        }

        let mut sel = Select::new();
        let recv_ptr = Box::into_raw(Vec::into_boxed_slice(receivers));
        let recv_slice : &'static [Receiver<Option<Msg>>] = unsafe { &*recv_ptr } ;
        for r in recv_slice { 
            sel.recv(r);
        }

        MultiBackend {
            last_recvd: None,
            continue_listening,
            sel,
            backends,
            receivers : recv_slice,
            receivers_ptr: recv_ptr,
        }
    }
}
impl<T: Ipc> Drop for MultiBackend<T> {
    fn drop(&mut self) {
        // clear the select
        std::mem::replace(&mut self.sel, Select::new());
        // recover the box, but don't drop it just yet so we can clear the slice first
        let _b = unsafe { Box::from_raw(self.receivers_ptr) };
        // clear the slice
        std::mem::replace(&mut self.receivers, &[]);
        // now we can drop the box safely
    }
}

impl<T: Ipc> Backend<T> for MultiBackend<T> {
    fn sender(&self) -> BackendSender<T> {
        match self.last_recvd {
            Some(i) => self.backends[i as usize].clone(),
            None    => panic!("Called sender but no messages have been received yet!")
        }
    }

    /// Return a copy of the flag variable that indicates that the
    /// `Backend` should continue listening (i.e., not exit).
    fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool> {
        Arc::clone(&(self.continue_listening))
    }

    fn next(&mut self) -> Option<Msg> {
        let oper = self.sel.select();
        let index = oper.index();
        self.last_recvd = Some(index);
        oper.recv(&self.receivers[index]).unwrap() // TODO don't just unwrap
    }
}


/// Backend will yield incoming IPC messages forever via `next()`.
/// It owns the socket; `BackendSender` holds weak references.
/// The atomic bool is a way to stop iterating.
pub struct SingleBackend<T: Ipc> {
    sock: Arc<T>,
    continue_listening: Arc<atomic::AtomicBool>,
    receive_buf: [u8; 1024],
    tot_read: usize,
    read_until: usize,
}

use crate::serialize::Msg;
impl<T: Ipc> Backend<T> for SingleBackend<T> {
    fn sender(&self) -> BackendSender<T> {
        BackendSender(Arc::downgrade(&self.sock))
    }

    /// Return a copy of the flag variable that indicates that the
    /// `Backend` should continue listening (i.e., not exit).
    fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool> {
        Arc::clone(&(self.continue_listening))
    }

    /// Get the next IPC message.
    // This is similar to `impl Iterator`, but the returned value is tied to the lifetime
    // of `self`, so we cannot implement that trait.
    fn next(&mut self) -> Option<Msg> {
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

}

impl<T: Ipc> SingleBackend<T> {
    pub fn new(
        sock: T,
        continue_listening: Arc<atomic::AtomicBool>,
    ) -> Self {
        SingleBackend {
            sock: Arc::new(sock),
            continue_listening,
            receive_buf: [0u8; 1024],
            tot_read: 0,
            read_until: 0,
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

impl<T: Ipc> Drop for SingleBackend<T> {
    fn drop(&mut self) {
        Arc::get_mut(&mut self.sock)
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
