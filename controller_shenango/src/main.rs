extern crate clap;
extern crate libc;
extern crate shenango;
extern crate cluster_message_types;
extern crate time;

use std::io::ErrorKind;
use std::net::SocketAddrV4;
use std::str::FromStr;

use clap::{App, Arg};

use cluster_message_types::{summary::Summary, allocation::Allocation};

mod backend;
use backend::Backend;

pub struct Controller {
    socket : backend::UdpConnection,
}

impl Controller {
    fn on_summary(&mut self, sum : &Summary, alloc: &mut Allocation) {
        alloc.id = sum.id;
        alloc.rate = 125_000 * 10;
    }

    fn run(&mut self) {
        let mut rcv_buf = [0u8; 1024];
        let mut sum : Summary = Default::default();
        let mut send_buf = [0u8; 128];
        let mut alloc : Allocation = Default::default();

        loop {
            match self.socket.recv_from(&mut rcv_buf) {
                Ok((amt, ref sender_addr)) => {
                    if amt > 0 {
												println!("got summary from {}", sum.id);
                        sum.read_from(&rcv_buf[..amt]);
                        self.on_summary(&sum, &mut alloc);
                        alloc.write_to(&mut send_buf);
                        match self.socket.send_to(&send_buf, *sender_addr) {
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
            .possible_values(&[
                "linux",
                "shenango"
            ])
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
        "shenango" => Backend::Shenango,
        _          => unreachable!(),
    };
    let config = matches.value_of("config");

    backend.init_and_run(config, move || {
        let socket = backend.listen_udp(addr);
        println!("Bound to {}. Listening...", socket.local_addr());

        let mut controller = Controller {
            socket
        };
        controller.run();
    });


}
