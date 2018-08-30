#![feature(integer_atomics)]
extern crate portus;
extern crate serde;
extern crate rmp_serde as rmps;
extern crate time;

use portus::serialize::summary::Summary;
use portus::serialize::allocation::Allocation;
use std::net::{SocketAddr, UdpSocket};
use std::io::ErrorKind;
use std::collections::HashMap;
use std::cmp::min;

pub struct Controller {
    cwnd: u32,
    init_cwnd: u32,
    min_rtt: u32,
    mss: u32,
    rate: u32,
    
    total_flows: u32,
    hosts: HashMap<u32, Host>,

    alpha : u32,
    beta : u32,
}

pub struct Host {
    rate: u32,
    flows: u32,
}

impl Controller {
    fn on_summary(&mut self, sum : &Summary, alloc: &mut Allocation) {
        if self.min_rtt <= 0 || sum.min_rtt < self.min_rtt {
            self.min_rtt = sum.min_rtt;
        }

        let in_queue = (((self.cwnd as f64) * ((sum.rtt - self.min_rtt) as f64)) / ((sum.rtt * self.mss) as f64)) as u32;
        if in_queue <= self.alpha  && in_queue > 0 {
            self.cwnd += self.mss;
        } else if in_queue >= self.beta {
            self.cwnd -= self.mss;
        }

        self.cwnd -= self.mss * sum.num_drop_events;
        if self.cwnd < self.init_cwnd {
            self.cwnd = self.init_cwnd;
        }

        /*
        let old_rate = self.rate;
        self.rate = (self.cwnd as f64 / (self.min_rtt as f64 / 1_000_000.0)) as u32;
        let rate_cap = self.rate - old_rate;

        let host = self.hosts.entry(sum.id).or_insert(Host { flows: 0, rate: 0});
        self.total_flows += sum.num_active_flows - host.flows;
        host.flows = sum.num_active_flows;
        host.rate = min(host.rate + rate_cap, self.rate * (sum.num_active_flows / self.total_flows));

        alloc.rate =  host.rate;
        alloc.id = sum.id;
        */
    }
}

fn main() {
    let addr = String::from("0.0.0.0:4052").parse::<SocketAddr>().unwrap();
    let socket = UdpSocket::bind(addr).expect("failed to bind to udp socket");

    let mut rcv_buf = [0u8; 1024];
    let mut sum : Summary = Default::default();
    let mut send_buf = [0u8; 128];
    let mut alloc : Allocation = Default::default();
    //let mut start_time = 0;
    let mut controller = Controller {
        alpha : 2, 
        beta  : 5,
        cwnd: 14480,
        init_cwnd: 14480,
        mss: 1448,
        min_rtt: 0,
        rate: 0,
        total_flows: 0,
        hosts : HashMap::new(),
    };

    loop {
        match socket.recv_from(&mut rcv_buf) {
            Ok((amt, ref sender_addr)) => {
                if amt > 0 {
                    sum.read_from(&rcv_buf[..amt]);
                    controller.on_summary(&sum, &mut alloc);
                    alloc.write_to(&mut send_buf);
                    match socket.send_to(&send_buf, sender_addr) {
                        Ok(_) => {}
                        Err(e) => { println!("send failed! {:?}", e); }
                    }
                }
            }   
            Err(ref err) if err.kind() != ErrorKind::WouldBlock => { panic!("UDP socket error: {}", err); }
            Err(_) => { panic!("UDP socket error: unknown"); }
        }
    }

}
/*

                    if start_time == 0 {
                        start_time = time::precise_time_ns();
                    }
                    let elapsed = (time::precise_time_ns() - start_time) / 1_000_000_000;
                    if elapsed > 40 {
                        rate = 10;
                    } else if elapsed > 30 {
                        rate = 20;
                    } else if elapsed > 20 {
                        rate = 30;
                    } else if elapsed > 10 {
                        rate = 20;
                    } else if elapsed > 0 {
                        rate = 10;
                    }
*/
