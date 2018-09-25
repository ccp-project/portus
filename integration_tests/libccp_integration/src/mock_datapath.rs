#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
include!(concat!(env!("OUT_DIR"), "/libccp.rs"));

use std;
use std::sync::mpsc;
use failure;
use minion;
use slog;
use time;

fn current_time() -> u64 {
    let now = time::now_utc().to_timespec();
    now.sec as u64 * 1_000_000 + now.nsec as u64 / 1_000
}

#[repr(C)]
pub struct mock_state {
    pub mock_cwnd: u32,
    pub mock_rate: u32,
}

#[repr(C)]
pub struct dp_impl(pub std::os::unix::net::UnixDatagram);

static mut TIME_ZERO: u64 = 0;

extern "C" fn mock_datapath_set_cwnd(_dp: *mut ccp_datapath, conn: *mut ccp_connection, cwnd: u32) {
    use std::os::raw::c_void as void;
    unsafe {
        let conn_state = std::mem::transmute::<*mut void, *mut mock_state>((*conn).impl_);
        (*conn_state).mock_cwnd = cwnd;
    }
}

extern "C" fn mock_datapath_set_rate_rel(_dp: *mut ccp_datapath, conn: *mut ccp_connection, rate_factor: u32) {
    use std::os::raw::c_void as void;
    unsafe {
        let conn_state = std::mem::transmute::<*mut void, *mut mock_state>((*conn).impl_);
        (*conn_state).mock_rate *= rate_factor;
    }
}

extern "C" fn mock_datapath_set_rate_abs(_dp: *mut ccp_datapath, conn: *mut ccp_connection, rate: u32) {
    use std::os::raw::c_void as void;
    unsafe {
        let conn_state = std::mem::transmute::<*mut void, *mut mock_state>((*conn).impl_);
        (*conn_state).mock_rate = rate;
    }
}

extern "C" fn mock_datapath_now() -> u64 { 
    unsafe { current_time() - TIME_ZERO }
}

extern "C" fn mock_datapath_since_usecs(then: u64) -> u64 {
    let now = mock_datapath_now();
    if then > now {
        return 0;
    }

    now - then
}

extern "C" fn mock_datapath_after_usecs(usecs: u64) -> u64 {
    let now = mock_datapath_now();
    now + usecs
}

extern "C" fn mock_datapath_send_msg(
    dp: *mut ccp_datapath, 
    _conn: *mut ccp_connection, 
    msg: *mut std::os::raw::c_char,
    msg_size: std::os::raw::c_int,
) -> std::os::raw::c_int { unsafe {
    let sk = std::mem::transmute::<*mut std::os::raw::c_void, *mut dp_impl>((*dp).impl_);
    let buf = std::slice::from_raw_parts(msg as *const u8, msg_size as usize);
    match (*sk).0.send_to(buf, "/tmp/ccp/0/in") {
        Err(_) => return -1,
        _ => return buf.len() as std::os::raw::c_int,
    }
} }

unsafe fn mock_primitives(i: u32, ccp_conn: *mut ccp_connection) {
    use std::os::raw::c_void as void;
    let curr_conn_state = std::mem::transmute::<*mut void, *const mock_state>((*ccp_conn).impl_);

    (*ccp_conn).prims.packets_acked = i;
    (*ccp_conn).prims.rtt_sample_us = 2;
    (*ccp_conn).prims.bytes_acked = 5;
    (*ccp_conn).prims.packets_misordered = 10;
    (*ccp_conn).prims.bytes_misordered = 100;
    (*ccp_conn).prims.lost_pkts_sample = 52;
    (*ccp_conn).prims.packets_in_flight = 100;
    (*ccp_conn).prims.rate_outgoing = 30;
    (*ccp_conn).prims.rate_incoming = 20;
    (*ccp_conn).prims.snd_cwnd = (*curr_conn_state).mock_cwnd;
    (*ccp_conn).prims.snd_rate = (*curr_conn_state).mock_rate as u64;
}

unsafe fn setup_mock_ccp_datapath(send_sk: std::os::unix::net::UnixDatagram) -> Option<ccp_datapath> {
    let sk_holder = Box::new(dp_impl(send_sk));
    let mut dp = ccp_datapath {
        set_cwnd: Some(mock_datapath_set_cwnd),
        set_rate_abs: Some(mock_datapath_set_rate_abs),
        set_rate_rel: Some(mock_datapath_set_rate_rel),
        time_zero: TIME_ZERO,
        now: Some(mock_datapath_now),
        since_usecs: Some(mock_datapath_since_usecs),
        after_usecs: Some(mock_datapath_after_usecs),
        send_msg: Some(mock_datapath_send_msg),
        impl_: Box::into_raw(sk_holder) as *mut std::os::raw::c_void,
    };

    let ok = ccp_init(&mut dp);
    if ok < 0 {
        println!("Could not initialize ccp datapath");
        return None;
    }

    Some(dp)
}

unsafe fn init_mock_connection() -> Option<*mut ccp_connection> {
    let mut dp_info = ccp_datapath_info {
        init_cwnd: 1500 * 10,
        mss: 1500,
        src_ip: 0,
        src_port: 1,
        dst_ip: 3,
        dst_port: 4,
        congAlg: ['\0' as i8 ; 64],
    };

    let mut m_state = mock_state {
        mock_cwnd: 1500,
        mock_rate: 0,
    };

    use std::os::raw::c_void as void;
    let ccp_conn = ccp_connection_start(std::mem::transmute::<*mut mock_state, *mut void>(&mut m_state), &mut dp_info);
    if ccp_conn.is_null() {
        return None;
    }

    Some(ccp_conn)
}

struct ccp_connection_wrapper(*mut ccp_connection);
unsafe impl std::marker::Send for ccp_connection_wrapper {}

struct ccpInvokeService(Option<mpsc::Sender<()>>, ccp_connection_wrapper, slog::Logger);

impl minion::Cancellable for ccpInvokeService {
    type Error = failure::Error;

    fn for_each(&mut self) -> Result<minion::LoopState, Self::Error> { unsafe { 
        if let Some(s) = self.0.take() {
            s.send(()).unwrap();
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        let conn_wrapper = &mut self.1;
        mock_primitives(52, conn_wrapper.0);

        let ok = ccp_invoke(conn_wrapper.0);
        if ok != 0 {
            return Ok(minion::LoopState::Continue);
        }

        Ok(minion::LoopState::Continue)
    } }
}

struct readMsgService(Option<mpsc::Sender<()>>, std::os::unix::net::UnixDatagram, Vec<u8>, slog::Logger);

impl minion::Cancellable for readMsgService {
    type Error = failure::Error;

    fn for_each(&mut self) -> Result<minion::LoopState, Self::Error> {
        if let Some(s) = self.0.take() {
            s.send(()).unwrap();
        }

        let read = match self.1.recv(&mut self.2) {
            Ok(r) => r,
            _ => return Ok(minion::LoopState::Continue),
        };

        let ok = unsafe { ccp_read_msg(self.2[..read].as_mut_ptr() as *mut std::os::raw::c_char, read as i32) };
        if ok < 0 {
            bail!("ccp_read_msg could not parse message: {}", ok);
        }

        Ok(minion::LoopState::Continue)
    }
}

fn listen_and_run(
    stop: mpsc::Receiver<()>, 
    ready: mpsc::Sender<()>,
    recv_sk: std::os::unix::net::UnixDatagram,
    ccp_conns: &[*mut ccp_connection], 
    log: slog::Logger,
) {
    // spawn two threads:
    // 1. recvfrom -> ccp_read_msg thread
    // 2. ccp_invoke thread
    use minion::Cancellable;
    let (msg_reader_ready_tx, msg_reader_ready_rx) = mpsc::channel();
    let msg_reader = readMsgService(
        Some(msg_reader_ready_tx), 
        recv_sk, 
        vec![0u8; 1024],
        log.clone(),
    ).spawn();
    let (ccp_invoker_ready_tx, ccp_invoker_ready_rx) = mpsc::channel();

    let mut ccp_invokers = vec![];
    for mut ccp_conn in ccp_conns {
        ccp_invokers.push(ccpInvokeService(
            Some(ccp_invoker_ready_tx.clone()), 
            ccp_connection_wrapper(*ccp_conn), 
            log.clone(),
        ).spawn());
    }

    // when both services are spawned, we are ready
    msg_reader_ready_rx.recv().unwrap();
    for _ in ccp_conns {
        ccp_invoker_ready_rx.recv().unwrap();
    }
    ready.send(()).unwrap();

    trace!(log, "running mock datapath...");

    // cancel and end both when the test is over
    stop.recv().unwrap();
    msg_reader.cancel();
    for ccp_invoker in ccp_invokers.iter() {
        ccp_invoker.cancel();
    }

    msg_reader.wait().unwrap();
    for ccp_invoker in ccp_invokers {
        ccp_invoker.wait().unwrap();
    }
}

pub fn start(
    stop: mpsc::Receiver<()>, 
    ready: mpsc::Sender<()>, 
    recv_sk: std::os::unix::net::UnixDatagram,
    num_connections: usize, 
    log: slog::Logger,
) {
    unsafe { TIME_ZERO = current_time(); }
    let send_sk = std::os::unix::net::UnixDatagram::unbound().unwrap();
    unsafe { setup_mock_ccp_datapath(send_sk).unwrap(); }

    let mut ccp_conns = vec![];
    for _ in 0..num_connections {
        ccp_conns.push(unsafe {
            let cn = init_mock_connection().expect("Error initializing mock datapath connection");
            mock_primitives(52, cn);
            cn
        })
    }

    listen_and_run(stop, ready, recv_sk, &ccp_conns, log);
}
