#[macro_use]
extern crate portus;

use portus::{CongAlg, DropEvent, Measurement};
use portus::pattern;
use portus::ipc::{Ipc, Backend};

struct Reno<T: Ipc> {
    control_channel: Option<Backend<T>>,
    sock_id: u32,
    ss_thresh: u32,
    cwnd: u32,
    last_ack: u32,
    init_cwnd: u32,
}

impl<T: Ipc> Default for Reno<T> {
    fn default() -> Self {
        Reno {
            control_channel: None,
            sock_id: Default::default(),
            ss_thresh: Default::default(),
            cwnd: Default::default(),
            last_ack: Default::default(),
            init_cwnd: Default::default(),
        }
    }
}

impl<T: Ipc> Reno<T> {
    fn send_pattern(&self) {
        let _ = self.control_channel.as_ref().map(|ch| {
            ch.send_pattern(
                self.sock_id,
                make_pattern!(
                    pattern::Event::SetCwndAbs(self.cwnd) => 
                    pattern::Event::WaitNs(1000) => 
                    pattern::Event::Report
                ),
            )
        });
    }
}

impl<T: Ipc> CongAlg<T> for Reno<T> {
    fn name(&self) -> String {
        String::from("reno")
    }

    fn create(&mut self, control: Backend<T>, sock_id: u32, start_seq: u32, init_cwnd: u32) {
        self.control_channel = Some(control);
        self.sock_id = sock_id;
        self.last_ack = start_seq;
        self.cwnd = init_cwnd;
        self.ss_thresh = 0x7fff;
        self.init_cwnd = init_cwnd;

        self.send_pattern();
    }

    fn measurement(&mut self, _sock_id: u32, m: Measurement) {
        // Handle integer overflow / sequence wraparound
        let mut new_bytes_acked = if m.ack < self.last_ack {
            (u32::max_value() - self.last_ack) + m.ack
        } else {
            m.ack - self.last_ack
        };

        self.last_ack = m.ack;
        if self.cwnd < self.ss_thresh {
            // increase cwnd by 1 per packet, until ssthresh
            if self.cwnd + new_bytes_acked > self.ss_thresh {
                new_bytes_acked -= self.ss_thresh - self.cwnd;
                self.cwnd = self.ss_thresh;
            } else {
                self.cwnd += new_bytes_acked;
                new_bytes_acked = 0;
            }
        }

        // increase cwnd by 1 / cwnd per packet
        self.cwnd += 1460u32 * new_bytes_acked / self.cwnd;
        self.send_pattern();

        println!("got ack: {} cwnd: {}", m.ack, self.cwnd);
    }

    fn drop(&mut self, _sock_id: u32, d: DropEvent) {
        match d {
            DropEvent::DupAck => {
                self.cwnd /= 2;
                if self.cwnd <= self.init_cwnd {
                    self.cwnd = self.init_cwnd;
                }

                self.ss_thresh = self.cwnd;
                println!("got dupack drop: cwnd {}", self.cwnd);
            }
            DropEvent::Timeout => {
                self.ss_thresh /= 2;
                self.cwnd = self.init_cwnd;
                println!("got timeout drop: cwnd {}", self.cwnd);
            }
            DropEvent::Ecn => println!("got ecn"),
        }

        self.send_pattern();
    }
}

fn main() {
    use portus::ipc::netlink::Socket;
    let b = Socket::new().and_then(|sk| Backend::new(sk)).expect(
        "ipc initialization",
    );

    portus::start::<_, Reno<Socket>>(b);
}
