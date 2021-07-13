use crossbeam::channel;

use super::Error;
use super::Result;
use std::marker::PhantomData;

pub struct Socket<T> {
    send: Option<channel::Sender<Vec<u8>>>,
    recv: Option<channel::Receiver<Vec<u8>>>,
    _phantom: PhantomData<T>,
}

impl<T> Socket<T> {
    pub fn new(to_ccp: channel::Sender<Vec<u8>>, from_ccp: channel::Receiver<Vec<u8>>) -> Self {
        Socket {
            send: Some(to_ccp),
            recv: Some(from_ccp),
            _phantom: PhantomData::<T>,
        }
    }

    fn __name() -> String {
        String::from("channel")
    }

    fn __send(&self, msg: &[u8]) -> Result<()> {
        let s = self
            .send
            .as_ref()
            .ok_or_else(|| Error(String::from("Send channel side missing")))?;
        s.send(msg.to_vec())?;
        Ok(())
    }

    fn __close(&mut self) -> Result<()> {
        self.send.take();
        self.recv.take();
        Ok(())
    }
}

use super::Blocking;
impl super::Ipc for Socket<Blocking> {
    type Addr = ();

    fn name() -> String {
        Self::__name()
    }

    fn send(&self, msg: &[u8], _to: &Self::Addr) -> Result<()> {
        self.__send(msg)
    }

    fn recv(&self, msg: &mut [u8]) -> Result<(usize, Self::Addr)> {
        let r = self
            .recv
            .as_ref()
            .ok_or_else(|| Error(String::from("Receive channel side missing")))?;
        let buf = r.recv_timeout(std::time::Duration::from_secs(1))?;
        msg[..buf.len()].copy_from_slice(&buf);
        Ok((buf.len(), ()))
    }

    fn close(&mut self) -> Result<()> {
        self.__close()
    }
}

use super::Nonblocking;
impl super::Ipc for Socket<Nonblocking> {
    type Addr = ();

    fn name() -> String {
        Self::__name()
    }

    fn send(&self, msg: &[u8], _to: &Self::Addr) -> Result<()> {
        self.__send(msg)
    }

    fn recv(&self, msg: &mut [u8]) -> Result<(usize, Self::Addr)> {
        let r = self
            .recv
            .as_ref()
            .ok_or_else(|| Error(String::from("Receive channel side missing")))?;
        let buf = r.try_recv()?;
        msg[..buf.len()].copy_from_slice(&buf);
        Ok((buf.len(), ()))
    }

    fn close(&mut self) -> Result<()> {
        self.__close()
    }
}

#[cfg(test)]
mod tests {
    use super::Socket;
    use crate::ipc::{Blocking, Ipc};
    use crossbeam::channel;
    use std::thread;

    #[test]
    fn basic() {
        let (tx, rx) = channel::unbounded();
        let (s1, r1) = channel::unbounded();
        let (s2, r2) = channel::unbounded();
        let ipc = Socket::<Blocking>::new(s1, r2);

        thread::spawn(move || {
            s2.send(vec![0, 9, 1, 8]).unwrap();
            let x = r1.recv().unwrap();
            assert_eq!(x, vec![0, 9, 1, 8]);
            tx.send(()).unwrap();
        });

        let mut buf = [0u8; 8];
        let (l, _) = ipc.recv(&mut buf).unwrap();
        ipc.send(&buf[..l], &()).unwrap();
        rx.recv().unwrap();
    }
}
