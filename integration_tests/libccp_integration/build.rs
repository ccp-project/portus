extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    let mut libccp_make = std::process::Command::new("make")
        .current_dir("./libccp")
        .spawn()
        .expect("libccp make failed");
    libccp_make
        .wait()
        .expect("libccp make spawned but failed");

    println!("cargo:rustc-link-search=./libccp");
    println!("cargo:rustc-link-lib=ccp");

    let bindings = bindgen::Builder::default()
        .header("./libccp/ccp.h")
        .whitelist_function(r#"ccp_\w+"#)
        .blacklist_type(r#"u\d+"#)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("libccp.rs"))
        .expect("Unable to write bindings");
}
