#![recursion_limit = "128"]

extern crate colored;
extern crate libc;
extern crate libloading;
extern crate portus;
extern crate proc_macro2;
extern crate quote;
extern crate regex;
extern crate structopt;
extern crate syn;
extern crate toml;
extern crate walkdir;

use colored::Colorize;
use walkdir::WalkDir;
//use clap::{App, Arg};
use std::process::Command;

#[derive(Debug)]
struct Alg {
    crate_name: String,
    crate_path: String,
}

use toml::Value;

fn log(action: &str, msg: &str) {
    println!(
        "{action:>12} {msg}",
        action = action.green().bold(),
        msg = msg
    );
}
fn log_warn(msg: &str) {
    eprintln!("{}: {}", "warning".yellow().bold(), msg);
}
fn log_error(msg: &str) -> ! {
    eprintln!("{}: {}", "error".red().bold(), msg);
    std::process::exit(1);
}

fn find_algs(root: PathBuf) -> Vec<Alg> {
    WalkDir::new(root)
        .max_depth(1)
        .min_depth(1)
        .into_iter()
        .filter_map(|entry| {
            let entry = match entry {
                Ok(ent) => ent,
                Err(err) => {
                    log_warn(&format!("Unable to open entry {:#?}", err));
                    return None;
                }
            };

            if !entry.file_type().is_dir() {
                return None;
            }

            if entry.file_name() == "lib" {
                return None;
            }

            //let crate_name = entry.path().file_name().unwrap().to_str().unwrap();
            let crate_path = entry.path().to_str().unwrap();
            let config_path = Path::new(entry.path()).join("Cargo.toml");
            let config_str = std::fs::read_to_string(config_path.clone()).unwrap_or_else(|_| {
                log_error(&format!("error reading {}", config_path.to_string_lossy()))
            });

            let config_toml: Value = toml::from_str(&config_str).unwrap_or_else(|_| {
                log_error(&format!("error parsing {}", config_path.to_string_lossy()))
            });

            let crate_name = &config_toml["package"]["name"].as_str().unwrap();
            Some(Alg {
                crate_name: crate_name.to_string(),
                crate_path: crate_path.to_owned(),
            })
        })
        .collect()
}

fn generate_cargo_toml(algs: &[Alg]) -> String {
    let toml = std::iter::once(
        r#"[package]
name = "startccp"
version = "0.1.0"
edition = "2018"

[lib]
name = "startccp"
crate-type = ["staticlib", "cdylib"]

[dependencies]
libc = "0.2"
clap = "2.32"
portus = "^0.5"
"#
        .to_owned(),
    );

    let algs_strs = algs.iter().map(
        |Alg {
             crate_name,
             crate_path,
         }| { format!("{} = {{ path = \"{}\" }}\n", crate_name, crate_path).to_string() },
    );

    itertools::join(toml.chain(algs_strs), "")
}

use proc_macro2::{Ident, Span};
use quote::*;

fn generate_lib_rs(algs: &[Alg]) -> String {
    let imports = algs.iter().map(|Alg { crate_name, .. }| {
        let name = Ident::new(crate_name, Span::call_site());
        quote! {
            extern crate #name;
        }
    });

    let loads = algs.iter().map(
        |Alg { crate_name, .. }| {
            let name = Ident::new(crate_name, Span::call_site());
            let name_key = Ident::new(&format!("{}_key", crate_name), Span::call_site());
            let name_args = Ident::new(&format!("{}_args", crate_name), Span::call_site());
            quote! {
                let #name_key = <#name::__ccp_alg_export as CongAlg<portus::ipc::chan::Socket<portus::ipc::Blocking>>>::name();
                let #name_args = #name::__ccp_alg_export::args();
            }
        },
    );

    let matches = algs.iter().map(|Alg{ crate_name, .. }| {
        let name = Ident::new(crate_name, Span::call_site());
        let name_key = Ident::new(&format!("{}_key", crate_name), Span::call_site());
        let name_args = Ident::new(&format!("{}_args", crate_name), Span::call_site());
        quote! {
            ref a if a == &#name_key => {
                let args = #name_args.arg(ipc_arg);
                let matches = args.get_matches_from(argv);
                let ipc = matches.value_of("ipc").unwrap();
                let alg = #name::__ccp_alg_export::with_arg_matches(&matches, Some(log.clone())).unwrap();
                portus::start!(ipc, Some(log), alg).unwrap()
            }
        }
    });

    let alglist = algs.iter().map(|Alg { crate_name, .. }| {
        let name_key = Ident::new(&format!("{}_key", crate_name), Span::call_site());
        quote! {
            eprintln!("- {}", #name_key);
        }
    });

    let lib_rs = quote! {
        extern crate clap;
        extern crate portus;
        #(#imports)*

        use clap::Arg;
        use portus::{CongAlg, CongAlgBuilder};

        use libc::c_char;
        use std::ffi::CStr;
        use std::os::unix::io::FromRawFd;
        use std::fs::File;

        fn _start(args: String, out: Option<File>) -> u32 {
            let argv = args.split_whitespace();
            let log = out.map_or_else(
                || portus::algs::make_logger(),
                |f| portus::algs::make_file_logger(f)
            );

            #(#loads)*

            let ipc_arg = Arg::with_name("ipc")
                .long("ipc")
                .help("Sets the type of ipc to use: (netlink|unix)")
                .default_value("unix")
                .validator(portus::algs::ipc_valid);

            let alg_name = argv.clone().next().expect("empty argument string");

            match alg_name {
                #(#matches)*
                _ => {
                    eprintln!("error: algorithm '{}' not found. available algorithms are: ", alg_name);
                    #(#alglist)*
                    return 1;
                }
            }
        }

        #[no_mangle]
        pub extern "C" fn libstartccp_run_forever(c_args: *const c_char, log_fd: i32) -> u32 {
            let args = unsafe { CStr::from_ptr(c_args) }.to_string_lossy().into_owned();
            let f = unsafe { File::from_raw_fd(log_fd) };
            _start(args, Some(f))
        }
    };

    let parsed = syn::parse2::<proc_macro2::TokenStream>(lib_rs.clone());
    if parsed.is_err() {
        log_error("error creating library");
    }

    lib_rs.to_string()
}

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

fn write_file(path: &PathBuf, code: String) {
    let mut file = match File::create(path) {
        Err(why) => {
            log_error(&format!(
                "unable to create file {}: {}",
                path.to_string_lossy().red().bold(),
                why
            ));
        }
        Ok(f) => f,
    };

    if let Err(why) = file.write_all(code.as_bytes()) {
        log_error(&format!(
            "unable to write file {}: {}",
            path.to_string_lossy().red().bold(),
            why
        ));
    }
}

fn rebuild_library(root: &PathBuf, algs: Vec<Alg>) -> bool {
    let lib_path = Path::new(root).join("lib");

    let cargo_path = lib_path.clone().join("Cargo.toml");
    write_file(&cargo_path, generate_cargo_toml(&algs));

    let librs_path = lib_path.clone().join("src").join("lib.rs");
    write_file(&librs_path, generate_lib_rs(&algs));

    let cargo_bin_cmd = Command::new("which")
        .arg("cargo")
        .output()
        .expect("unable to find cargo, make sure it is in your path");
    let cargo_bin = std::str::from_utf8(&cargo_bin_cmd.stdout).unwrap().trim();
    if cargo_bin == "" {
        log_error("unable to find cargo, make sure it is in your path")
    }

    Command::new("sudo")
        .arg(cargo_bin)
        .arg("+stable")
        .arg("fmt")
        .current_dir(lib_path.clone())
        .output()
        .expect("failed to run rustfmt on ccp lib");

    let build_status = Command::new("sudo")
        .arg(cargo_bin)
        .arg("build")
        .arg("--release")
        .current_dir(lib_path.clone())
        .status()
        .expect("failed to build ccp lib with cargo");

    if !build_status.success() {
        return false;
    }

    let orig_path = lib_path
        .clone()
        .join("target")
        .join("release")
        .join("libstartccp.so");
    let link_path = "/usr/lib/libstartccp.so";

    log(
        "Linking",
        &format!("{} -> {}", orig_path.to_string_lossy(), link_path),
    );

    Command::new("sudo")
        .arg("ln")
        .arg("-s")
        .arg(orig_path.clone())
        .arg(link_path)
        .output()
        .expect("failed to link into /usr/lib");

    true
}

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(
    name = "ccp",
    author = "",
    raw(setting = "clap::AppSettings::DeriveDisplayOrder")
)]
/// Manage and run congestion control algorithms built on CCP
struct Ccp {
    #[structopt(short = "d", long = "dir", parse(from_os_str))]
    /// Where to store algorithms (defaults to /usr/local/ccp)
    dir: Option<PathBuf>,
    #[structopt(subcommand)]
    cmd: Subcommand,
}

static RUN_HELP: &str = r#"
USAGE:
    ccp run <algorithm> -- [args]
"#;

#[derive(Debug, StructOpt)]
enum Subcommand {
    #[structopt(name = "get", author = "")]
    /// Download and build a new algorithm
    Get {
        #[structopt(name = "url")]
        /// Where to find the algorithm. May be a normal URL or one of the following shortcuts:{n}
        ///     (1) name of a ccp-project, eg. "reno"{n}
        ///     (2) github_user/repo, eg. "venkatarun95/copa"{n}
        ///
        /// NOTE: Currently only supports git repositories.{n}
        /// If you use a different VCS or have any trouble fetching algorithms with ccp get,{n}
        /// you can manually place the algorithm code in the root ccp directory (e.g ~/.ccp){n}
        /// and then run `ccp makelib`.
        url: String,
        #[structopt(name = "branch", long = "branch")]
        /// Use a specific branch for this repository rather than master
        branch: Option<String>,
    },
    #[structopt(name = "list", author = "")]
    /// List the currently available algorithms
    List {},
    #[structopt(name = "run", author = "", raw(help = "RUN_HELP"))]
    /// Start a CCP algorithm
    Run {
        #[structopt(name = "algorithm")]
        alg: String,
    },
    #[structopt(name = "makelib", author = "")]
    /// Force-rebuild the ccp algorithms library
    Makelib {},
}

use std::io::ErrorKind;

fn main() {
    let opt = Ccp::from_iter(std::env::args().take_while(|a| a != "--"));

    // Our working directory. ~/.ccp unless user says otherwise
    let root = match opt.dir {
        Some(p) => p,
        None => PathBuf::from("/usr/local/ccp"),
    };

    // Prepare environment. Make sure root directory exists, is accessible, has a dir called lib/
    let res = std::fs::create_dir(root.clone());
    match res {
        Ok(_) => {}
        Err(ref error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => {
            log_error(&format!(
                "unable to create root directory {}: {:#?}",
                root.clone().to_string_lossy().red().bold(),
                error
            ));
        }
    };

    let lib_dir = Path::new(&root).join("lib").join("src");
    let res = std::fs::create_dir_all(lib_dir.clone());
    match res {
        Ok(_) => {}
        Err(ref error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => {
            log_error(&format!(
                "unable to create lib directory {}: {:#?}",
                lib_dir.clone().to_string_lossy().red().bold(),
                error
            ));
        }
    };

    let alg_paths = find_algs(root.clone());

    match opt.cmd {
        Subcommand::Get { url, branch } => {
            use regex::RegexSet;
            let set =
                RegexSet::new(&[r#"^[a-zA-Z0-9-._]+$"#, r#"^[a-zA-Z0-9-]+/[a-zA-Z0-9-._]+$"#])
                    .unwrap();
            let mut url = match set.matches(&url).into_iter().collect::<Vec<_>>().get(0) {
                Some(0) => format!("https://github.com/ccp-project/{}", url),
                Some(1) => format!("https://github.com/{}", url),
                None => url,
                Some(_) => unreachable!(),
            };

            if &url[url.len() - 4..] != ".git" {
                url = format!("{}.git", url);
            }

            let branch = branch.unwrap_or_else(|| String::from("master"));

            log("Cloning", &format!("{} --branch {}", url, branch));

            let url_clone = url.clone();
            let dir_name = url_clone
                .split('/')
                .last()
                .unwrap()
                .split('.')
                .next()
                .unwrap();
            let old_dir = root.clone().join(dir_name);

            Command::new("sudo")
                .arg("rm")
                .arg("-rf")
                .arg(old_dir.clone())
                .output()
                .expect("rm");

            let _ = Command::new("git")
                .current_dir(root.clone())
                .arg("clone")
                .arg("--recurse-submodules")
                .arg(url)
                .arg("--branch")
                .arg(branch)
                .output()
                .expect("git clone");

            let alg_paths = find_algs(root.clone());
            let res = rebuild_library(&root, alg_paths);
            if !res {
                Command::new("sudo")
                    .arg("rm")
                    .arg("-rf")
                    .arg(old_dir)
                    .output()
                    .expect("rm");
                log_error("Failed to rebuild library. This is most likely becasue the algorithm does not implement the CongAlgBuilder trait or simply has a bug.")
            }
        }
        Subcommand::List {} => {
            for Alg {
                crate_name,
                crate_path,
            } in alg_paths
            {
                let remote_cmd = Command::new("git")
                    .arg("remote")
                    .arg("get-url")
                    .arg("origin")
                    .current_dir(crate_path.clone())
                    .output()
                    .expect("git remote failed");
                let remote = std::str::from_utf8(&remote_cmd.stdout).unwrap().trim();

                let branch_cmd = Command::new("git")
                    .arg("rev-parse")
                    .arg("--abbrev-ref")
                    .arg("HEAD")
                    .current_dir(crate_path.clone())
                    .output()
                    .expect("git rev-parse failed");
                let branch = std::str::from_utf8(&branch_cmd.stdout).unwrap().trim();

                println!(
                    "- {} @ {} (url={}, branch={})",
                    crate_name.blue().bold(),
                    crate_path,
                    remote,
                    branch
                );
            }
        }
        Subcommand::Run { alg } => {
            let after_dash = std::env::args()
                .skip_while(|a| a != "--")
                .collect::<Vec<String>>();

            let argv = if !after_dash.is_empty() {
                after_dash[1..].join(" ")
            } else {
                String::from("")
            };

            log("Running", &format!("{} {}", alg, argv));

            use libc::c_char;
            use std::ffi::CString;
            let lib_path = Path::new(&root)
                .join("lib")
                .join("target")
                .join("release")
                .join("libstartccp.so");
            let lib = libloading::Library::new(lib_path).expect("failed to load dynamic library");
            unsafe {
                let spawn: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> u32> = lib
                    .get(b"libstartccp_run_forever")
                    .expect("failed to get spawn function");
                let args = CString::new(format!("{} {}", alg, argv)).expect("CString::new failed");
                spawn(args.as_ptr());
            }
        }
        Subcommand::Makelib {} => {
            rebuild_library(&root, alg_paths);
        }
    };
}
