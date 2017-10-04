use std;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

extern crate nix;

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

pub struct Backend<T: Ipc + Sync> {
    sock: Arc<T>,
    notif_ch: mpsc::Sender<Vec<u8>>,
    close: Arc<std::sync::atomic::AtomicBool>,
}

impl<T: Ipc + Sync> Clone for Backend<T> {
    fn clone(&self) -> Self {
        Backend {
            sock: self.sock.clone(),
            notif_ch: self.notif_ch.clone(),
            close: self.close.clone(),
        }
    }
}

impl<T: Ipc + 'static + Sync + Send> Backend<T> {
    // Pass in a T: Ipc, the Ipc substrate to use.
    // Return a Backend on which to call send_msg
    // and a channel on which to listen for incoming
    pub fn new(sock: T) -> Result<(Backend<T>, mpsc::Receiver<Vec<u8>>), Error> {
        let (tx, rx): (mpsc::Sender<Vec<u8>>, mpsc::Receiver<Vec<u8>>) = mpsc::channel();
        let b = Backend {
            sock: Arc::new(sock),
            notif_ch: tx,
            close: Default::default(),
        };

        b.listen();
        Ok((b, rx))
    }

    // Blocking send.
    pub fn send_msg(&self, addr: Option<u16>, msg: &[u8]) -> Result<(), Error> {
        self.sock.send(addr, msg).map_err(|e| Error::from(e))
    }

    fn listen(&self) {
        let me = self.clone();
        thread::spawn(move || {
            let mut rcv_buf = vec![0u8; 1024];
            while me.close.load(Ordering::SeqCst) {
                let len = match me.sock.recv(rcv_buf.as_mut_slice()) {
                    Ok(l) => l,
                    Err(e) => {
                        println!("{:?}", e);
                        continue;
                    }
                };

                rcv_buf.truncate(len);
                match me.notif_ch.send(rcv_buf.clone()) {
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

impl<T: Ipc + Sync> Drop for Backend<T> {
    fn drop(&mut self) {
        // tell the receive loop to exit
        self.close.store(true, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use std;
    use std::thread;

    #[test]
    fn test_unix() {
        let (tx, rx) = std::sync::mpsc::channel();
        let c1 = thread::spawn(move || {
            let sk1 = super::unix::Socket::new(0).expect("init socket");
            let (_, r1) = super::Backend::new(sk1).expect("init backend");
            tx.send(true).expect("chan send");
            let msg = r1.recv().expect("receive message"); // Vec<u8>
            let got = std::str::from_utf8(&msg[..]).expect("parse message to str");
            assert_eq!(got, "hello, world");
        });

        let c2 = thread::spawn(move || {
            rx.recv().expect("chan rcv");
            let sk2 = super::unix::Socket::new(42424).expect("init socket");
            let (b2, _) = super::Backend::new(sk2).expect("init backend");
            b2.send_msg(None, "hello, world".as_bytes()).expect(
                "send message",
            );
        });

        c2.join().expect("join sender thread");
        c1.join().expect("join rcvr thread");
    }
}
