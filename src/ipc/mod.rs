use std;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

extern crate nix;

#[derive(Debug)]
pub struct Error(String);

impl From<nix::Error> for Error {
    fn from(e: nix::Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(format!("err {}", e))
    }
}

#[cfg(all(linux))]
pub mod netlink;
pub mod unix;

pub trait Ipc {
    fn send(&self, addr: Option<u16>, msg: &[u8]) -> Result<(), Error>; // Blocking send
    fn recv(&self, msg: &mut [u8]) -> Result<usize, Error>; // Blocking listen
    fn close(&self) -> Result<(), Error>; // Close the underlying sockets
}

pub struct Backend<T: Ipc + Sync> {
    sock: Arc<T>,
    close: Arc<std::sync::atomic::AtomicBool>,
}

impl<T: Ipc + Sync> Clone for Backend<T> {
    fn clone(&self) -> Self {
        Backend {
            sock: self.sock.clone(),
            close: self.close.clone(),
        }
    }
}

impl<T: Ipc + 'static + Sync + Send> Backend<T> {
    // Pass in a T: Ipc, the Ipc substrate to use.
    // Return a Backend on which to call send_msg
    // and listen
    pub fn new(sock: T) -> Result<Backend<T>, Error> {
        Ok(Backend {
            sock: Arc::new(sock),
            close: Default::default(), // initialized to false
        })
    }

    // Blocking send.
    pub fn send_msg(&self, addr: Option<u16>, msg: &[u8]) -> Result<(), Error> {
        self.sock.send(addr, msg).map_err(|e| Error::from(e))
    }

    // Start listening on the IPC socket
    // Return a channel on which incoming messages will be passed
    pub fn listen(&self) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) = mpsc::channel();
        let me = self.clone();
        thread::spawn(move || {
            let mut rcv_buf = vec![0u8; 1024];
            while !me.close.load(Ordering::SeqCst) {
                let len = match me.sock.recv(rcv_buf.as_mut_slice()) {
                    Ok(l) => l,
                    Err(_) => {
                        //println!("{:?}", e);
                        continue;
                    }
                };

                if len == 0 {
                    continue;
                }

                rcv_buf.truncate(len);
                let _ = tx.send(rcv_buf.clone());
            }
        });

        rx
    }
}

impl<T: Ipc + Sync> Drop for Backend<T> {
    fn drop(&mut self) {
        // tell the receive loop to exit
        self.close.store(true, Ordering::SeqCst)
    }
}

#[cfg(test)]
pub mod test;
