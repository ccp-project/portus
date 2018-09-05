include!(concat!(env!("OUT_DIR"), "/libccp.rs"));

extern crate time;

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
) -> std::os::raw::c_int {
    unimplemented!();
}

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

fn main() {
    unsafe {
        TIME_ZERO = current_time();
    }

    unsafe {
        setup_mock_ccp_datapath().unwrap();
    }
}
