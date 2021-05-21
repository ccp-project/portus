use anyhow::{bail, Error};
use minion::Cancellable;
use slog::Logger;
use std::sync::Arc;

use super::ACKED_PRIMITIVE;

pub struct MockDatapath {
    pub sk: crossbeam::channel::Sender<Vec<u8>>,
    pub logger: Logger,
}

impl libccp::DatapathOps for MockDatapath {
    fn send_msg(&mut self, msg: &[u8]) {
        self.sk.send(msg.to_vec()).unwrap_or_else(|_| ())
    }
}

pub struct DatapathMessageReader(Arc<libccp::Datapath>, crossbeam::channel::Receiver<Vec<u8>>);

impl Cancellable for DatapathMessageReader {
    type Error = Error;

    fn for_each(&mut self) -> Result<minion::LoopState, Self::Error> {
        let mut read = match self.1.recv_timeout(std::time::Duration::from_millis(1_000)) {
            Ok(r) => r,
            Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                return Ok(minion::LoopState::Continue);
            }
            Err(e) => bail!(e),
        };

        self.0.recv_msg(&mut read[..])?;
        Ok(minion::LoopState::Continue)
    }
}

pub struct MockConnectionState {
    pub mock_cwnd: u32,
    pub mock_rate: u32,
}

impl libccp::CongestionOps for MockConnectionState {
    fn set_cwnd(&mut self, cwnd: u32) {
        self.mock_cwnd = cwnd;
    }

    fn set_rate_abs(&mut self, rate: u32) {
        self.mock_rate = rate;
    }
}

// Do not change the order of these fields.
// It is important that libccp::Connection is dropped before Arc<libccp::Datapath>,
// since the last drop of Arc<libccp:Datapath> will cause it to be dropped, which
// frees memory that libccp::Connection contains a pointer to.
// See https://github.com/rust-lang/rfcs/blob/master/text/1857-stabilize-drop-order.md for drop
// order documentation.
// This compiles because we promised Rust (via mem::transmute) that the
// libccp::Connection has lifetime 'static.
pub struct MockConnection(
    libccp::Connection<'static, MockConnectionState>,
    Arc<libccp::Datapath>,
);

// For the same reason, impl Drop to prevent split borrows, which could cause the
// fields of MockConnection to get dropped independently.
impl Drop for MockConnection {
    fn drop(&mut self) {}
}

impl Cancellable for MockConnection {
    type Error = Error;

    fn for_each(&mut self) -> Result<minion::LoopState, Self::Error> {
        std::thread::sleep(std::time::Duration::from_millis(100));

        let (c, r) = { (self.0.mock_cwnd, self.0.mock_rate) };

        self.0.load_primitives(
            libccp::Primitives::default()
                .with_packets_acked(0)
                .with_rtt_sample_us(2)
                .with_bytes_acked(ACKED_PRIMITIVE)
                .with_packets_misordered(10)
                .with_bytes_misordered(100)
                .with_lost_pkts_sample(52)
                .with_packets_in_flight(100)
                .with_rate_outgoing(30)
                .with_rate_incoming(20)
                .with_snd_cwnd(c)
                .with_snd_rate(r as u64),
        );

        self.0.invoke()?;

        Ok(minion::LoopState::Continue)
    }
}

pub fn start(
    log: Logger,
    num_connections: usize,
    ipc_sender: crossbeam::channel::Sender<Vec<u8>>,
    ipc_receiver: crossbeam::channel::Receiver<Vec<u8>>,
) -> (minion::Handle<Error>, Vec<minion::Handle<Error>>) {
    let dp = MockDatapath {
        sk: ipc_sender,
        logger: log.clone(),
    };

    let dp = libccp::Datapath::init(dp).unwrap();
    let dp = Arc::new(dp);

    let conns: Vec<MockConnection> = (0..num_connections)
        .into_iter()
        .map(|_| dp.clone())
        .map(move |dp_local| {
            let c = MockConnectionState {
                mock_cwnd: 0,
                mock_rate: 0,
            };

            let mc = libccp::Connection::start(
                &dp_local,
                c,
                libccp::FlowInfo::default()
                    .with_mss(1500)
                    .with_init_cwnd(15_000)
                    .with_four_tuple(0, 1, 2, 3),
            )
            .unwrap();

            MockConnection(unsafe { std::mem::transmute(mc) }, dp_local.clone())
        })
        .collect();

    let dpr = DatapathMessageReader(dp, ipc_receiver);
    let msg_reader = dpr.spawn();

    let handles: Vec<minion::Handle<Error>> = conns.into_iter().map(|conn| conn.spawn()).collect();
    (msg_reader, handles)
}
