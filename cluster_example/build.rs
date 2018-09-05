extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
	println!("cargo:rustc-link-lib=nl-genl-3");
	println!("cargo:rustc-link-lib=nfnetlink");
	println!("cargo:rustc-link-lib=nl-route-3");
	println!("cargo:rustc-link-lib=nl-3");

	let bindings = bindgen::Builder::default()
		.header("nl-route.h")
		.clang_arg("-I/usr/include/libnl3")
		.whitelist_function("nl_socket_alloc")
		.whitelist_function("nl_connect")
		.whitelist_function("rtnl_link_alloc_cache")
		.whitelist_function("rtnl_link_get_by_name")
		.whitelist_function("rtnl_link_get_ifindex")
		.whitelist_function("rtnl_qdisc_alloc_cache")
		.whitelist_function("rtnl_qdisc_alloc")
		.whitelist_function("rtnl_qdisc_get")
		.whitelist_function("rtnl_tc_get_stat")
		.whitelist_function("rtnl_qdisc_put")
		.whitelist_function("TC_CAST")
		.whitelist_function("TC_HANDLE")
		.generate()
		.expect("unable to generate netlink-route bindings");
	let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
	bindings.write_to_file(out_path.join("bindings.rs"))
		.expect("couldn't write bindings!");
}
