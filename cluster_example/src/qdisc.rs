use std;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

pub struct Qdisc {
    sock: *mut nl_sock,
    qdisc: *mut rtnl_qdisc
}

use std::ffi::CString;
impl Qdisc {
    pub fn new(if_name: String, (tc_maj, tc_min): (u32, u32)) -> Self {
        unsafe {
            let mut all_links: *mut nl_cache = std::mem::uninitialized();
            let mut all_qdiscs: *mut nl_cache = std::mem::uninitialized();

            let sock = nl_socket_alloc();
            nl_connect(sock, NETLINK_ROUTE as i32);

            let ret = rtnl_link_alloc_cache(sock, AF_UNSPEC as i32, &mut all_links);
            if ret < 0 {
                panic!(format!("rtnl_link_alloc_cache failed: {}", ret));
            }

            let link = rtnl_link_get_by_name(all_links, CString::new(if_name).unwrap().as_ptr());
            let ifindex = rtnl_link_get_ifindex(link);
            
            let ret2 = rtnl_qdisc_alloc_cache(sock, &mut all_qdiscs);
            if ret2 < 0 {
                panic!(format!("rtnl_qdisc_alloc_cache failed: {}", ret2));
            }
            let tc_handle = ((tc_maj << 16) & 0xFFFF0000) | (tc_min & 0x0000FFFF);
            eprintln!("handle {}", tc_handle);
            let qdisc = rtnl_qdisc_get(all_qdiscs, ifindex, tc_handle);
            if qdisc.is_null() {
                panic!("rtnl_qdisc_get failed")
            }

            Qdisc {
                sock,
                qdisc
            }
        }
    }

    pub fn set_rate(&self, rate: u32, burst: u32) -> Result<(), ()> {
        unsafe {
            rtnl_qdisc_tbf_set_rate(self.qdisc, rate as i32, burst as i32, 0);
            let ret = rtnl_qdisc_add(self.sock, self.qdisc, NLM_F_REPLACE as i32);
            if ret < 0 {
                return Err(())
            }
            Ok(())
        }
    }
}
impl Drop for Qdisc {
    fn drop(&mut self) {
        println!("dropping!");
        unsafe {
            rtnl_qdisc_put(self.qdisc);
        }
    }
}
