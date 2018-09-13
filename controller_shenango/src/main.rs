extern crate clap;
extern crate libc;
#[cfg(feature = "iokernel")]
extern crate shenango;
extern crate cluster_message_types;
extern crate time;

use std::io::ErrorKind;
use std::net::{SocketAddr, SocketAddrV4};
use std::str::FromStr;
use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};

use clap::{App, Arg};

use cluster_message_types::{summary::Summary, allocation::Allocation};

mod backend;
use backend::Backend;

const DEFAULT_SUMMARY_INTERVAL_MS: u32 = 23;
const DEFAULT_SUMMARY_SLACK_MS: u32 = 2;
const DEFAULT_BTL_BW_BPS: u32 = (96_000_000 / 8);
const NS_IN_MS: u32 = 1000000;

pub struct Sender {
    addr : SocketAddrV4,
    active: bool,
    num_active_flows : u32,
}
pub struct Controller {
    // listening socket (for summaries from ccps)
    socket : backend::UdpConnection,
    // how often ccp should send summaries, may want to adjust dynamically based on RTTs 
    summary_interval_ms: u32,
    // controller will collect all summaries received during (interval-slack, interval+slack) and then make an allocation decision and send to everyone
    summary_slack_ms: u32,
    // any summaries collected between start and end will be used for allocation
    summary_period_start: u64,
    summary_period_end: u64,

    senders : HashMap<u32, Sender>,    
    num_active_senders: u32,
    num_active_flows: u32,

    btl_bw_bps : u32,
}

impl Controller {
    fn on_summary(&mut self, sum : &Summary, from : SocketAddrV4) {

        match self.senders.entry(sum.id) {
            Vacant(e) => {
                e.insert(Sender {
                    addr : from,
                    active : true,
                    num_active_flows : sum.num_active_flows,
                });
            }
            Occupied(e) => {
                let sender = e.into_mut();
                if sender.active {
                    // sender has already sent us a summary in this period so just ignore this one
                    return; 
                } else {
                    sender.num_active_flows = sum.num_active_flows;
                    sender.active = true;
                }
            }
        }
        self.num_active_flows += sum.num_active_flows;
        self.num_active_senders += 1;
    }

    fn reallocate(&mut self, alloc : &mut Allocation) {
        for (id, sender) in self.senders.iter_mut() {
            if sender.active {
                alloc.id = *id;
                alloc.rate = ((self.btl_bw_bps as f32) * ((sender.num_active_flows as f32) / (self.num_active_flows as f32))) as u32;
                alloc.burst = alloc.rate / 200; // mostly arbitrary, needs to be at least rate/HZ and HZ is 250
                alloc.next_summary_in_ms = self.summary_interval_ms;
                match self.socket.send_to(alloc.as_slice(), sender.addr) {
                    Ok(_) => {}
                    Err(e) => { println!("send to {} failed! {:?}", sender.addr, e); }
                }
                sender.active = false;
            }
        }
        self.num_active_senders = 0;
        self.num_active_flows = 0;
    }

    fn reset_period(&mut self, now : u64) {
        self.summary_period_start = now + (self.summary_interval_ms * NS_IN_MS) as u64; //- (self.summary_slack_ms * NS_IN_MS);
        self.summary_period_end = now + ((self.summary_interval_ms * NS_IN_MS) + (self.summary_slack_ms * NS_IN_MS)) as u64;
    }

    fn run(&mut self) {
        //let mut rcv_buf = [0u8; 1024];
        let mut sum : Summary = Default::default();
        //let mut send_buf = [0u8; 128];
        let mut alloc : Allocation = Default::default();

        loop {
            let now = time::precise_time_ns();

            match self.socket.recv_from(sum.as_mut_slice()) {
                Ok((amt, ref sender_addr)) => {
                    if amt > 0 {
                        if self.num_active_senders == 0 {
                            self.on_summary(&sum, *sender_addr);
                            self.reallocate(&mut alloc);
                            self.reset_period(now);
                        } else if now >= self.summary_period_start && now <= self.summary_period_end {
                            self.on_summary(&sum, *sender_addr);
                        }
                    }
                }   
                Err(ref err) if err.kind() != ErrorKind::WouldBlock => { panic!("UDP socket error: {}", err); }
                Err(_) => { panic!("UDP socket error: unknown"); }
            }

            if now > self.summary_period_end {
                self.reallocate(&mut alloc);
                self.reset_period(now);
            }
        }
    }
}

fn main() {
    let matches = App::new("Cluster Congestion Controller")
        .version("0.1")
        .arg(
            Arg::with_name("addr")
                .index(1)
                .help("Address and port to listen on")
                .required(true),
        )
        .arg(
            Arg::with_name("backend")
            .long("backend")
            .takes_value(true)
            .required(true)
            .possible_values(
                if cfg!(feature = "iokernel") {
                    &["linux","shenango"]
                } else {
                    &["linux"]
                }
            )
            .requires_ifs(&[("shenango", "config")])
            .help("Which networking stack to use")
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .takes_value(true)
                .help("Path to shenango config file")
        )
        .get_matches();

    println!("Starting controller...");

    let addr: SocketAddrV4 = FromStr::from_str(matches.value_of("addr").unwrap()).unwrap();
    let backend = match matches.value_of("backend").unwrap() {
        "linux"    => Backend::Linux,
        #[cfg(feature = "iokernel")]
        "shenango" => Backend::Shenango,
        _          => unreachable!(),
    };
    let config = matches.value_of("config");

    backend.init_and_run(config, move || {
        let socket = backend.listen_udp(addr, true);
        println!("Bound to {}. Listening...", socket.local_addr());

        let mut controller = Controller {
            socket,
            summary_interval_ms: DEFAULT_SUMMARY_INTERVAL_MS,
            summary_slack_ms: DEFAULT_SUMMARY_SLACK_MS,
            summary_period_start: 0,
            summary_period_end: 0,

            senders : HashMap::new(),
            num_active_senders : 0,
            num_active_flows : 0,
            btl_bw_bps : DEFAULT_BTL_BW_BPS,
        };
        controller.run();
    });


}
