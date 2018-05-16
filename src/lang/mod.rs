//! The datapath program compiler.
//! 
//! Datapath programs consist of two parts:
//! 1. Variable definitions
//! 2. Event definitions
//!
//! Variable Definitions
//! --------------------
//!
//! The `def` keyword starts the variable definitions clause. It must appear at the beginning of
//! the program. A `def` clause contains one or more variable definitions, and the definition of
//! the `Report` struct. Only the variables within the `Report` struct will be accessible from CCP
//! programs. Variables within the `Report` struct can optionally be declared `volatile`, which
//! means they will be reset to their default values after each report is sent to CCP. For example, 
//! a variable counting the number of cumulatively acknowledged packets would be declared volatile
//! to prevent double-counting these values in the CCP algorithm logic.
//!
//! ### Example
//! ```no-run
//! (def 
//!     (state_var 0)
//!     (Report
//!         (minrtt +infinity)
//!         (volatile acked 0)
//!     )
//! )
//! ```
//!
//! Event Definitions
//! -----------------
//!
//! Then `when` keyword starts an event definition clause. There are one or more event definition
//! clauses in the datapath program. The datapath will evaluate each event definition clause in
//! order. The first expression following the `when` keyword must evaluate to a boolean value, if
//! it is true, then the body is evaluated, and subsequent events are ignored unless
//! `(fallthrough)` is specified.
//!
//! ### Example
//! ```no-run
//! (when true
//!     (:= Report.minrtt (min Report.minrtt Flow.rtt_sample_us))
//!     (fallthrough)
//! )
//! (when (> Micros 50)
//!     (report)
//! )
//! ```
//!
//! Compiling
//! ---------
//!
//! `lang::compile()` will take a byte array with datapath program source and produce a `Bin`,
//! which contains a series of instructions and can be serialized into a format libccp-compliant
//! datapaths understand.
//!
//! ### Example
//!
//! Let's compile a program which would count the number of ECN-marked packets over 1 millisecond intervals.
//!
//! ```
//! extern crate portus;
//! use portus::lang;
//!
//! fn main() {
//!     let my_cool_program = b"
//!         (def (Report (volatile ecnpackets 0)))
//!         (when true
//!             (:= Report.ecnpackets (+ Report.ecnpackets Ack.ecn_packets))
//!             (fallthrough)
//!         )
//!         (when (> Micros 1000)
//!             (report)
//!             (reset)
//!         )
//!     ";
//!     let (bin, scope) = lang::compile(my_cool_program).unwrap();
//! }
//! ```
//!
//! Available Primitives
//! --------------------
//!
//! The datapath makes available the following primitives:
//!
//!  Name                   | Description                 
//! ------------------------|-----------------------------
//! "Ack.bytes_acked"       | In-order bytes acked        
//! "Ack.bytes_misordered"  | Out-of-order bytes acked    
//! "Ack.ecn_bytes"         | ECN-marked bytes            
//! "Ack.ecn_packets"       | ECN-marked packets          
//! "Ack.lost_pkts_sample"  | Number of lost packets      
//! "Ack.now"               | Current time                
//! "Ack.packets_acked"     | In-order packets acked      
//! "Ack.packets_misordered"| Out-of-order packets acked  
//! "Flow.bytes_in_flight"  | Bytes in flight             
//! "Flow.bytes_pending"    | Bytes in socket buffer      
//! "Flow.packets_in_flight"| Packets in flight           
//! "Flow.rate_incoming"    | Incoming rate               
//! "Flow.rate_outgoing"    | Outgoing rate               
//! "Flow.rtt_sample_us"    | Round-trip time             
//! "Flow.was_timeout"      | Did a timeout occur?        

use std;
use nom;

use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct Error(pub String);
impl std::error::Error for Error {
    fn description(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(self.0.as_str())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
impl From<String> for Error {
    fn from(e: String) -> Error {
        Error(e)
    }
}
impl<'a> From<&'a str> for Error {
    fn from(e: &'a str) -> Error {
        Error(String::from(e))
    }
}
impl From<nom::simple_errors::Err> for Error {
    fn from(e: nom::simple_errors::Err) -> Error {
        Error(String::from(e.description()))
    }
}
impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Error {
        Error(format!("err {}", e))
    }
}
impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Error {
        Error(format!("err {}", e))
    }
}

mod ast;
mod datapath;
mod prog;
mod serialize;

pub use self::datapath::Bin;
pub use self::datapath::Type;
pub use self::datapath::Reg;
pub use self::datapath::Scope;
pub use self::prog::Prog;

/// `compile()` uses 5 passes to yield Instrs.
///
/// 1. `Expr::new()` (called by `Prog::new_with_scope()` internally) returns a single AST from
///    `src`
/// 2. `Prog::new_with_scope()` returns a list of ASTs for multiple expressions
/// 3. The ASTs are desugared to support (report) and (fallthrough).
/// 4. The list of runtime updates (from `updates`) for values is applied to the Scope.
/// 5. `Bin::compile_prog()` turns a `Prog` into a `Bin`, which is a `Vec` of datapath `Instr`
pub fn compile(src: &[u8], updates: &[(&str, u32)]) -> Result<(Bin, Scope)> {
    Prog::new_with_scope(src)
        .and_then(|(p, mut s)| {
            for &(name, new_val) in updates {
                println!("name: {}, new_val: {}", name, new_val);
                match s.update_type(name, &Type::Num(Some(new_val as u64))) {
                    Ok(_) => {},
                    Err(e) => println!("err: {}", e)
                }
                println!("done");
            }

            Ok((Bin::compile_prog(&p, &mut s)?, s))
        })
}

/// `compile_and_serialize()` adds a fourth pass.
/// The resulting bytes can be passed to the datapath.
///
/// `serialize::serialize()` serializes a `Bin` into bytes.
pub fn compile_and_serialize(src: &[u8], updates: &[(&str, u32)]) -> Result<(Vec<u8>, Scope)> {
    compile(src, updates).and_then(|(b, s)| Ok((b.serialize()?, s)))
}

#[cfg(test)]
mod tests {
    extern crate test;
    use self::test::Bencher;
    
    #[bench]
    fn bench_1_line_compileonly(b: &mut Bencher) {
        let fold = "
            (def (Report.foo 0))
            (when true
                (:= Report.foo (+ Report.foo Ack.bytes_acked))
            )
        ".as_bytes();
        b.iter(|| super::compile(fold, &[]).unwrap())
    }

    #[bench]
    fn bench_1_line(b: &mut Bencher) {
        let fold = "
            (def (Report.foo 0))
            (when true
                (:= Report.foo (+ Report.foo Ack.bytes_acked))
            )
        ".as_bytes();
        b.iter(|| super::compile_and_serialize(fold, &[]).unwrap())
    }
    
    #[bench]
    fn bench_2_line(b: &mut Bencher) {
        let fold = "
            (def (Report.foo 0) (Report.bar 0))
            (when true
                (:= Report.foo (+ Report.foo Ack.bytes_acked))
                (:= Report.bar (+ Report.bar Ack.bytes_misordered))
            )
        ".as_bytes();
        b.iter(|| super::compile_and_serialize(fold, &[]).unwrap())
    }
    
    #[bench]
    fn bench_ewma(b: &mut Bencher) {
        let fold = "
            (def (Report.foo 0) (Report.bar 0))
            (when true
                (:= Report.foo (+ Report.foo Ack.bytes_acked))
                (:= Report.bar (ewma 2 Flow.rate_outgoing))
            )
        ".as_bytes();
        b.iter(|| super::compile_and_serialize(fold, &[]).unwrap())
    }
    
    #[bench]
    fn bench_if(b: &mut Bencher) {
        let fold = "
            (def (Report.foo 0) (Report.bar false))
            (when true
                (:= Report.foo (+ Report.foo Ack.bytes_acked))
                (bind Report.bar (!if Report.bar (> Ack.lost_pkts_sample 0)))
            )
        ".as_bytes();
        b.iter(|| super::compile_and_serialize(fold, &[]).unwrap())
    }
    
    #[bench]
    fn bench_3_line(b: &mut Bencher) {
        let fold = "
            (def (Report.foo 0) (Report.bar 0) (Control.baz 0))
            (when true
                (:= Report.foo (+ Report.foo Ack.bytes_acked))
                (:= Report.bar (+ Report.bar Ack.bytes_misordered))
                (:= Report.baz (+ Report.bar Ack.ecn_bytes))
            )
        ".as_bytes();
        b.iter(|| super::compile_and_serialize(fold, &[]).unwrap())
    }
}
