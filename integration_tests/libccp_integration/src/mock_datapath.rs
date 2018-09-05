#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
include!(concat!(env!("OUT_DIR"), "/libccp.rs"));

use std;
use std::os::unix::net::UnixDatagram;
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

static mut TIME_ZERO: u64 = 0;
static mut m_state: mock_state = mock_state {
    mock_cwnd: 1500,
    mock_rate: 0,
};
static mut send_sk: Option<UnixDatagram> = None;

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
    now - then
}

extern "C" fn mock_datapath_after_usecs(usecs: u64) -> u64 {
    let now = mock_datapath_now();
    now + usecs
}

extern "C" fn mock_datapath_send_msg(
    _dp: *mut ccp_datapath, 
    _conn: *mut ccp_connection, 
    msg: *mut std::os::raw::c_char,
    msg_size: std::os::raw::c_int,
) -> std::os::raw::c_int { unsafe {
    if let Some(ref mut sk) = send_sk {
        if !std::path::Path::new("/tmp/ccp/0/in").exists() {
            return -3;
        }

        let buf = std::slice::from_raw_parts(msg as *const u8, msg_size as usize);
        sk.send_to(buf, "/tmp/ccp/0/in")
            .map(|d| d as i32)
            .unwrap_or_else(|_| -1) 
            as std::os::raw::c_int
    } else {
        return -2;
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

unsafe fn setup_mock_ccp_datapath() -> Option<ccp_datapath> {
    let mut dp = ccp_datapath {
        set_cwnd: Some(mock_datapath_set_cwnd),
        set_rate_abs: Some(mock_datapath_set_rate_abs),
        set_rate_rel: Some(mock_datapath_set_rate_rel),
        time_zero: TIME_ZERO,
        now: Some(mock_datapath_now),
        since_usecs: Some(mock_datapath_since_usecs),
        after_usecs: Some(mock_datapath_after_usecs),
        send_msg: Some(mock_datapath_send_msg),
        impl_: std::ptr::null_mut(),
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

        let conn_wrapper = &mut self.1;
        mock_primitives(52, conn_wrapper.0);

        let ok = ccp_invoke(conn_wrapper.0);
        if ok != 0 {
            bail!("ccp_invoke failed: {}", ok);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        Ok(minion::LoopState::Continue)
    } }
}

struct readMsgService(Option<mpsc::Sender<()>>, UnixDatagram, Vec<u8>, slog::Logger);

impl minion::Cancellable for readMsgService {
    type Error = failure::Error;

    fn for_each(&mut self) -> Result<minion::LoopState, Self::Error> {
        if let Some(s) = self.0.take() {
            s.send(()).unwrap();
        }

        let read = self.1.recv(&mut self.2[..]);
        let read = match read {
            Ok(r) if r > 0 => r,
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
    ccp_conn: *mut ccp_connection, 
    log: slog::Logger,
) {
    // clear old unix socket
    std::fs::create_dir_all("/tmp/ccp/0").unwrap_or_else(|_| ());
    std::fs::remove_file("/tmp/ccp/0/out").unwrap_or_else(|_| ());

    // make receiving socket
    let recv_sk = UnixDatagram::bind("/tmp/ccp/0/out").unwrap();
    recv_sk.set_read_timeout(Some(std::time::Duration::from_millis(1000))).unwrap();

    // spawn two threads:
    // 1. recvfrom -> ccp_read_msg thread
    // 2. ccp_invoke thread
    use minion::Cancellable;
    let (msg_reader_ready_tx, msg_reader_ready_rx) = mpsc::channel();
    let msg_reader = readMsgService(
        Some(msg_reader_ready_tx), 
        recv_sk, 
        vec![0u8; 32678], 
        log.clone(),
    ).spawn();
    let (ccp_invoker_ready_tx, ccp_invoker_ready_rx) = mpsc::channel();
    let ccp_invoker = ccpInvokeService(
        Some(ccp_invoker_ready_tx), 
        ccp_connection_wrapper(ccp_conn), 
        log.clone(),
    ).spawn();

    // when both services are spawned, we are ready
    msg_reader_ready_rx.recv().unwrap();
    ccp_invoker_ready_rx.recv().unwrap();
    ready.send(()).unwrap();

    debug!(log, "running mock datapath...");

    // cancel and end both when the test is over
    stop.recv().unwrap();
    msg_reader.cancel();
    ccp_invoker.cancel();
    msg_reader.wait().unwrap();
    ccp_invoker.wait().unwrap();
}

pub fn start(stop: mpsc::Receiver<()>, ready: mpsc::Sender<()>, log: slog::Logger) {
    unsafe { TIME_ZERO = current_time(); }
    unsafe { setup_mock_ccp_datapath().unwrap(); }
    unsafe { send_sk = Some(UnixDatagram::unbound().unwrap()); }
    let ccp_conn = unsafe {
        let cn = init_mock_connection().expect("Error initializing mock datapath connection");
        mock_primitives(52, cn);
        cn
    };

    listen_and_run(stop, ready, ccp_conn, log);
}
