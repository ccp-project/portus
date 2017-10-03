use std::sync::mpsc;
use std::thread;

extern crate nix;
extern crate libc;

#[derive(Debug)]
pub struct Error(String);

impl std::convert::From<nix::Error> for Error {
    fn from(e: nix::Error) -> Error {
        Error(format!("err {}", e))
    }
}

impl std::convert::From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error(format!("err {}", e))
    }
}

pub mod netlink;
pub mod unix;

pub trait Ipc {
    fn send(&self, addr: Option<u16>, msg: &[u8]) -> Result<(), Error>; // Blocking send
    fn recv(&self, msg: &mut [u8]) -> Result<usize, Error>; // Blocking listen
    fn close(&self) -> Result<(), Error>; // Close the underlying sockets
}

pub struct Backend<T: Ipc> {
    sock: T,
    notif_ch: mpsc::Sender<Box<[u8]>>,
}

impl<T: Ipc + Clone + 'static + Send> Backend<T> {
    // Pass in a T: Ipc, the Ipc substrate to use.
    // Return a Backend on which to call send_msg
    // and a channel on which to listen for incoming
    pub fn new(sock: T) -> Result<(Backend<T>, mpsc::Receiver<Box<[u8]>>), Error> {
        let (tx, rx): (mpsc::Sender<Box<[u8]>>, mpsc::Receiver<Box<[u8]>>) = mpsc::channel();
        let b = Backend {
            sock: sock,
            notif_ch: tx,
        };

        b.listen();
        Ok((b, rx))
    }

    // Blocking send.
    pub fn send_msg(&self, addr: Option<u16>, msg: &[u8]) -> Result<(), Error> {
        self.sock.send(addr, msg).map_err(|e| Error::from(e))
    }

    fn listen(&self) {
        let notif_sender = self.notif_ch.clone();
        let sk = self.sock.clone();
        thread::spawn(move || {
            let mut rcv_buf = vec![0u8; 1024];
            loop {
                match sk.recv(rcv_buf.as_mut_slice()) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("{:?}", e);
                        continue;
                    }
                };

                match notif_sender.send(rcv_buf.clone().into_boxed_slice()) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("{}", e);
                        continue;
                    }
                };
            }
        });
    }
}

impl<T: Ipc> Drop for Backend<T> {
    fn drop(&mut self) {
        self.sock.close().is_ok();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
