extern crate portus;
extern crate quote;
extern crate syn;
extern crate walkdir;

use std::env::args;
use std::fs::File;
use std::io::Read;

use portus::lang;
use quote::ToTokens;
use syn::punctuated::{Pair::End, Pair::Punctuated};
use syn::visit::Visit;
use syn::{Expr, Expr::Lit, Expr::MethodCall, Item::Impl, Lit::ByteStr, Lit::Str};
use walkdir::{DirEntry, WalkDir};

const ESC: &str = "\u{1B}";
const RED: &str = "\031";
const GREEN: &str = "\032";
const BLUE: &str = "\034";

macro_rules! bold_red {
    ($s:expr) => {
        format!("{}[{};1m{}{}[0m", ESC, RED, $s, ESC)
    };
}
macro_rules! bold_blue {
    ($s:expr) => {
        format!("{}[{};1m{}{}[0m", ESC, BLUE, $s, ESC)
    };
}
macro_rules! bold_green {
    ($s:expr) => {
        format!("{}[{};1m{}{}[0m", ESC, GREEN, $s, ESC)
    };
}
macro_rules! bold {
    ($s:expr) => {
        format!("{}[1m{}{}[0m", ESC, $s, ESC)
    };
}

struct FastPathProgramFinder {
    total: u32,
    failed: u32,
    impl_str: String,
    filename: String,
}
impl FastPathProgramFinder {
    fn new(impl_str: String, filename: String) -> Self {
        Self {
            total: 0,
            failed: 0,
            impl_str,
            filename,
        }
    }
}
impl<'v> Visit<'v> for FastPathProgramFinder {
    fn visit_expr(&mut self, e: &Expr) {
        if let MethodCall(ref emc) = *e {
            let method_name = emc.method.to_string();
            if method_name == "install" {
                match emc.args.first() {
                    Some(Punctuated(&Lit(ref l), _)) | Some(End(&Lit(ref l))) => {
                        let compile_result = match l.lit {
                                ByteStr(ref ls) => { lang::compile(&ls.value(), &[]) }
                                Str(ref ls)     => { lang::compile(ls.value().as_bytes(), &[]) }
                                _           => { panic!("Non-string passed to install(). This shouldn't have compiled in the first place...") }
                            };
                        self.total += 1;
                        match compile_result {
                            Ok(_) => {}
                            Err(e) => {
                                self.failed += 1;
                                eprintln!("{}{}", bold_red!("error"), bold!(format!(": {:?}", e)));
                                eprintln!("{} {}", bold_blue!("-->"), self.filename);
                                eprintln!("{} {}", bold_blue!("-->"), self.impl_str);
                                let prog_src = l.into_tokens().to_string();
                                eprintln!(
                                    "{}\n\n",
                                    prog_src
                                        .split('\n')
                                        .enumerate()
                                        .map(|(i, l)| format!(
                                            "{} {}",
                                            bold_blue!(format!("{:3} |", i)),
                                            l
                                        ))
                                        .collect::<Vec<String>>()
                                        .join("\n")
                                );
                            }
                        }
                    }
                    Some(Punctuated(&MethodCall(ref mcmc), _))
                    | Some(End(&MethodCall(ref mcmc))) => {
                        self.visit_expr(&mcmc.receiver);
                    }
                    _ => {}
                }
            }
            self.visit_expr(&emc.receiver)
        }
    }
}

const HELP_MSG: &str = r#"Tests compilation of fast-path programs

Usage:
    cargo compile-fast-path [--path PATH]

Options:
    -h, --help    Print this message
    --path        Root directory of files to check, assumes ./src
"#;

fn show_help() {
    eprintln!("{}", HELP_MSG);
}

fn main() {
    if args().any(|a| a == "--help" || a == "-h") {
        println!("help");
        show_help();
        return;
    }
    let num_args = args().len();
    if num_args != 2 && num_args != 4 {
        show_help();
        return;
    }
    let mut opts = args().skip(2);
    let path = {
        if opts.len() == 2 {
            if opts.next() != Some("--path".to_string()) {
                show_help();
                return;
            }
            opts.next().unwrap()
        } else {
            "./src".to_string()
        }
    };

    let walker = WalkDir::new(path.clone()).into_iter();
    fn is_hidden(entry: &DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    }
    fn is_dir(entry: &DirEntry) -> bool {
        entry.file_type().is_dir()
    }
    fn is_rs(entry: &DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            .unwrap()
            .to_string()
            .split('.')
            .last()
            .unwrap_or("")
            == "rs"
    }

    let mut total = 0;
    let mut failed = 0;

    for entry in walker
        .filter_entry(|e| !is_hidden(&e))
        .filter(|e| e.is_ok())
        .map(|e| e.unwrap())
        .filter(|e| !is_dir(e) && is_rs(e))
    {
        let filepath = &entry.path();
        let mut file = File::open(&filepath).expect("Unable to open file");
        let mut src = String::new();
        file.read_to_string(&mut src).expect("Unable to read file");
        let syntax = syn::parse_file(&src).expect("Unable to parse file");
        for item in syntax.items {
            match item {
                Impl(imp) => {
                    let struct_name = match *imp.self_ty {
                        syn::Type::Path(tp) => tp.path.segments[0].ident.to_string(),
                        _ => panic!("no struct name!"), // TODO better msg
                    };
                    let trait_name = match imp.trait_ {
                        Some(tr) => Some(tr.1.segments[0].ident.to_string()),
                        None => None,
                    };
                    let impl_str = match trait_name {
                        Some(tn) => format!("impl {} for {}", tn, struct_name),
                        None => format!("impl {}", struct_name),
                    };
                    let mut pf =
                        FastPathProgramFinder::new(impl_str, filepath.display().to_string());
                    for imp_item in imp.items {
                        pf.visit_impl_item(&imp_item);
                    }
                    total += pf.total;
                    failed += pf.failed;
                }
                _ => continue,
            }
        }
    }
    if total > 0 {
        if failed > 0 {
            eprintln!("{}{}", bold_red!("error"), bold!(format!(": {}/{} fast-path programs failed to compile.\n       You should resolve these issues before running the CCP.", failed, total)))
        } else {
            println!(
                "       {} {} fast-path programs in {}",
                bold_green!("Found"),
                total,
                path
            );
            println!(
                "{} {}",
                bold_green!("    Verified"),
                format!("{} programs compile successfully", total)
            );
        }
    } else {
        println!(
            "       {} 0 fast-path programs in {}",
            bold_green!("Found"),
            path
        );
    }
}
