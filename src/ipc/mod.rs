//! A library wrapping various IPC mechanisms with a datagram-oriented
//! messaging layer. This is how CCP communicates with the datapath.

use std::rc::{Rc, Weak};
use std::sync::{atomic, Arc};

use super::Error;
use super::Result;

use std::cell::RefCell;
use super::SocketStats;

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
///
/// This API enables both connection-oriented (send/recv) and connectionless (sendto/recvfrom)
/// sockets, but currently only unix sockets support connectionless sockets. When using unix
/// sockets, you must provide a valid `Addr` to `send()` and you will also receive a valid
/// `Addr` as a return value from `recv`. When using connection-oriented ipc mechanisms, these
/// values are ignored and should just be the nil value `()`. 
pub trait Ipc: 'static + Send {
    type Addr: Clone + Default + std::cmp::Eq + std::hash::Hash + std::fmt::Debug;
    /// Returns the name of this IPC mechanism (e.g. "netlink" for Linux netlink sockets)
    fn name() -> String;
    /// Blocking send
    fn send(&self, msg: &[u8], to: &Self::Addr) -> Result<()>;
    /// Blocking listen. 
    ///
    /// Returns how many bytes were read, and (if using unix sockets) the address of the sender. 
    ///
    /// Important: should not allocate!
    fn recv(&self, msg: &mut [u8]) -> Result<(usize,Self::Addr)>;
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
    pub fn build<'a>(
        self,
        atomic_bool: Arc<atomic::AtomicBool>,
        receive_buf: &'a mut [u8],
        stats: Rc<RefCell<SocketStats>>,
    ) -> Backend<'a, T> {
        Backend::new(self.sock, atomic_bool, receive_buf, stats)
    }
}

/// A send-only handle to the underlying IPC socket.
pub struct BackendSender<T: Ipc>(Weak<T>, T::Addr);

impl<T: Ipc> BackendSender<T> {
    /// Blocking send.
    pub fn send_msg(&self, msg: &[u8]) -> Result<()> {
        let s = Weak::upgrade(&self.0)
            .ok_or_else(|| Error(String::from("Send on closed IPC socket!")))?;
        s.send(msg, &self.1).map_err(Error::from)
    }
    pub fn clone_with_dest(&self, to: T::Addr) -> Self {
        BackendSender(self.0.clone(), to)
    }
}

impl<T: Ipc> Clone for BackendSender<T> {
    fn clone(&self) -> Self {
        BackendSender(self.0.clone(), self.1.clone())
    }
}

/// Backend will yield incoming IPC messages forever via `next()`.
/// It owns the socket; `BackendSender` holds weak references.
/// The atomic bool is a way to stop iterating.
pub struct Backend<'a, T: Ipc> {
    sock: Rc<T>,
    continue_listening: Arc<atomic::AtomicBool>,
    receive_buf: &'a mut [u8],
    tot_read: usize,
    read_until: usize,
    last_recv_addr: T::Addr,
    stats: Rc<RefCell<SocketStats>>,
}

use crate::serialize::Msg;
impl<'a, T: Ipc> Backend<'a, T> {
    pub fn new(
        sock: T,
        continue_listening: Arc<atomic::AtomicBool>,
        receive_buf: &'a mut [u8],
        stats: Rc<RefCell<SocketStats>>,
    ) -> Backend<'a, T> {
        Backend {
            sock: Rc::new(sock),
            continue_listening,
            receive_buf,
            tot_read: 0,
            read_until: 0,
            last_recv_addr: Default::default(),
            stats,
        }
    }

    pub fn sender(&self, to: T::Addr) -> BackendSender<T> {
        BackendSender(Rc::downgrade(&self.sock), to)
    }

    /// Return a copy of the flag variable that indicates that the
    /// `Backend` should continue listening (i.e., not exit).
    pub fn clone_atomic_bool(&self) -> Arc<atomic::AtomicBool> {
        Arc::clone(&(self.continue_listening))
    }

    /// Get the next IPC message.
    // This is similar to `impl Iterator`, but the returned value is tied to the lifetime
    // of `self`, so we cannot implement that trait.
    pub fn next(&mut self, logger: Option<&slog::Logger>) -> Option<(Msg<'_>, T::Addr)> {
        // if we have leftover buffer from the last read, parse another message.
        if self.read_until < self.tot_read {
            let (msg, consumed) = Msg::from_buf(&self.receive_buf[self.read_until..]).ok()?;
            self.read_until += consumed;
            Some((msg, self.last_recv_addr.clone()))
        } else {
            self.tot_read = self.get_next_read(logger).ok()?;
            self.read_until = 0;
            let (msg, consumed) =
                Msg::from_buf(&self.receive_buf[self.read_until..self.tot_read]).ok()?;
            self.read_until += consumed;

            self.stats.borrow_mut().maybe_print();
            Some((msg, self.last_recv_addr.clone()))
        }
    }

    // calls IPC repeatedly to read one or more messages.
    // Returns a slice into self.receive_buf covering the read data
    fn get_next_read(&mut self, logger: Option<&slog::Logger>) -> Result<usize> {
        loop {
            // if continue_loop has been set to false, stop iterating
            if !self.continue_listening.load(atomic::Ordering::SeqCst) {
                eprintln!("[ccp] recieved kill signal");
                return Err(Error(String::from("Done")));
            }

            let mut stats = self.stats.borrow_mut();
            stats.before_recv();
            let (read, addr) = match self.sock.recv(self.receive_buf) {
                Ok(r) => r,
                Err(Error(e)) => {
                    if let Some(log) = logger {
                        warn!(log, "recv failed";
                              "err" => format!("{:#?}", e)
                        );
                    }
                    continue;
                }
            };
            stats.after_recv();

            // NOTE This may seem precarious, but is safe
            // In the case that `recv` returns a buffer containing multiple messages,
            // `next()` will continue to hit the first `if` branch (and thus will not
            // call `get_next_read()` again) until all of the messages from that buffer 
            // have been returned. So it is not possible for recvs to interleave and 
            // interfere with the last_recv_addr value. 
            self.last_recv_addr = addr;

            if read == 0 {
                continue;
            }

            return Ok(read);
        }
    }
}

impl<'a, T: Ipc> Drop for Backend<'a, T> {
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
