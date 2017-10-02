use std::sync::mpsc;
use std::thread;

use super::*;

mod socket;

#[derive(Debug)]
pub struct Netlink {
    sock: socket::Socket,
    notif_ch: mpsc::Sender<Box<[u8]>>,
}

impl Netlink {
    fn listen(&self) {
        let notif_sender = self.notif_ch.clone();
        let sk = self.sock.clone();
        thread::spawn(move || {
            let mut rcv_buf = vec![0u8; 1024];
            loop {
                match sk.recv(rcv_buf.as_mut_slice()) {
                    Ok(_) => (),
                    Err(e) => {
                        println!("{}", e);
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

impl Ipc for Netlink {
    fn new(_: Option<u32>) -> Result<(Backend, mpsc::Receiver<Box<[u8]>>), Error> {
        let sock = socket::Socket::new(22)?;
        let (tx, rx): (mpsc::Sender<Box<[u8]>>, mpsc::Receiver<Box<[u8]>>) = mpsc::channel();
        let nl = Netlink {
            sock: sock,
            notif_ch: tx,
        };

        nl.listen();
        Ok((Backend::Nl(nl), rx))
    }

    fn send_msg(&self, msg: &[u8]) -> Result<(), Error> {
        self.sock.send(msg).map_err(|e| Error::from(e))
    }

    fn close(self) -> Result<(), Error> {
        self.sock.close().map_err(|e| Error::from(e))
    }
}
