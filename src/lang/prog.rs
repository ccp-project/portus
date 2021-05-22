use nom::types::CompleteByteSlice;
use nom::*;

use super::ast::{atom, comment, expr, exprs, name, Expr};
use super::datapath::{check_atom_type, Scope, Type};
use super::{Error, Result};

/// An `Event` is a condition expression and a sequence of execution expressions.
/// If the condition expression evaluates to `true`, the execution expressions are
/// evaluated.
/// Scope cascades through the `Expr`:
/// Expr with `Type::Name` will in scope for successive `Expr`
#[derive(Debug, PartialEq)]
pub struct Event {
    pub flag: Expr,
    pub body: Vec<Expr>,
}

/// AST representation of a datapath program.
#[derive(Debug, PartialEq)]
pub struct Prog(pub Vec<Event>);

// ------------------------------------------
// (def (decl)...) grammar
// ------------------------------------------

// Declare a state variable and provide an initial value
// Optionally declare the variable "volatile", meaning it gets reset on "(report)"
named_complete!(
    decl<(bool, Type, Type)>,
    ws!(delimited!(
        tag!("("),
        tuple!(
            map!(opt!(tag!("volatile")), |v: Option<CompleteByteSlice>| v
                .is_some()),
            map!(name, Type::Name),
            map_res!(atom, |a: Result<Expr>| a.and_then(|i| check_atom_type(&i)))
        ),
        tag!(")")
    ))
);
named_complete!(
    report_struct<Vec<(bool, Type, Type)>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(tag!("Report") >> d: many1!(decl) >> (d)),
        tag!(")")
    ))
);

// a Prog has special syntax *at the beginning* to declare variables.
// (def (decl) ...)
named_complete!(
    defs<Vec<(bool, Type, Type)>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            tag!("def")
                >> defs1: many0!(decl)
                >> reports: opt!(report_struct)
                >> defs2: many0!(decl)
                >> (reports
                    .into_iter()
                    .flat_map(std::iter::IntoIterator::into_iter)
                    .filter_map(|(is_volatile, name, init_val)| match name {
                        Type::Name(name) => Some(Type::Name(format!("Report.{}", name))),
                        _ => None,
                    }
                    .map(|full_name| match init_val {
                        x @ Type::Num(_) | x @ Type::Bool(_) => (is_volatile, full_name, x),
                        _ => (is_volatile, full_name, Type::None),
                    }))
                    .chain(
                        defs1
                            .into_iter()
                            .chain(defs2)
                            .map(|(is_volatile, name, init_val)| match init_val {
                                x @ Type::Num(_) | x @ Type::Bool(_) => (is_volatile, name, x),
                                _ => (is_volatile, name, Type::None),
                            })
                    )
                    .collect())
        ),
        tag!(")")
    ))
);

// ------------------------------------------
// (when (bool expr) (body)...) grammar
// ------------------------------------------

// (when (single expr) (expr)...)
named_complete!(
    event<Result<Event>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            tag!("when")
                >> c: expr
                >> body: exprs
                >> (c.and_then(|cond| {
                    let exps: Result<Vec<Expr>> = body.into_iter().collect();
                    Ok(Event {
                        flag: cond,
                        body: exps?,
                    })
                }))
        ),
        tag!(")")
    ))
);
named_complete!(
    events<Vec<Result<Event>>>,
    many1!(do_parse!(opt!(comment) >> e: event >> (e)))
);

fn get_error(src: CompleteByteSlice) -> Error {
    match events(src) {
        Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Error::from(e),
        _ => Error::from("none"),
    }
}

impl Prog {
    /// Turn raw bytes into an AST representation, including implementing syntactic sugar features
    /// such as `(report)` and `(fallthrough)`.
    pub fn new_with_scope(source: &[u8]) -> Result<(Self, Scope)> {
        let mut scope = Scope::new();
        let body = match defs(CompleteByteSlice(source)) {
            Ok((rest, flow_state)) => {
                let (reports, controls): (Vec<(bool, String, Type)>, Vec<(bool, String, Type)>) =
                    flow_state
                        .into_iter()
                        .map(|(is_volatile, var, typ)| match var {
                            Type::Name(v) => (is_volatile, v, typ),
                            _ => unreachable!(),
                        })
                        .partition(|&(_, ref var, _)| var.starts_with("Report."));

                for (is_volatile, var, typ) in reports {
                    scope.new_report(is_volatile, var, typ);
                }

                for (is_volatile, var, typ) in controls {
                    scope.new_control(is_volatile, var, typ);
                }

                Ok(rest)
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Err(Error::from(e)),
            Err(nom::Err::Incomplete(Needed::Unknown)) => {
                Err(Error::from(String::from("need more src")))
            }
            Err(nom::Err::Incomplete(Needed::Size(s))) => {
                Err(Error::from(format!("need {} more bytes", s)))
            }
        }?;

        let evs = match events(body) {
            Ok((rest, _)) if !rest.is_empty() => {
                let e = get_error(rest);
                Err(Error::from(format!(
                    "compile error: \"{:?}\" in \"{}\"",
                    e,
                    std::str::from_utf8(rest.0)?
                )))
            }
            Ok((_, me)) => me.into_iter().collect(),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Err(Error::from(e)),
            Err(nom::Err::Incomplete(Needed::Unknown)) => Err(Error::from("need more src")),
            Err(nom::Err::Incomplete(Needed::Size(s))) => {
                Err(Error::from(format!("need {} more bytes", s)))
            }
        }?;

        let mut p = Prog(evs);
        p.desugar();

        // TODO make Expr::new return Iter, make self wrap an iter also
        Ok((p, scope))
    }

    fn desugar(&mut self) {
        self.0
            .iter_mut()
            .for_each(|v| v.body.iter_mut().for_each(Expr::desugar));
    }
}

#[cfg(test)]
mod tests {
    use nom;
    use nom::types::CompleteByteSlice;

    use crate::lang::ast::{Expr, Op, Prim};
    use crate::lang::datapath::{Scope, Type};
    use crate::lang::prog::{Event, Prog};

    #[test]
    fn defs() {
        let foo = b"(def (Bar 0) (Report (Foo 0) (volatile Baz 0)) (Qux 0) (volatile Qux2 0))";
        use nom::Needed;
        match super::defs(CompleteByteSlice(foo)) {
            Ok((r, me)) => {
                assert_eq!(r, CompleteByteSlice(&[]));
                assert_eq!(
                    me,
                    vec![
                        (
                            false,
                            Type::Name(String::from("Report.Foo")),
                            Type::Num(Some(0))
                        ),
                        (
                            true,
                            Type::Name(String::from("Report.Baz")),
                            Type::Num(Some(0))
                        ),
                        (false, Type::Name(String::from("Bar")), Type::Num(Some(0))),
                        (false, Type::Name(String::from("Qux")), Type::Num(Some(0))),
                        (true, Type::Name(String::from("Qux2")), Type::Num(Some(0))),
                    ]
                );
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => panic!("{:?}", e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn def_infinity() {
        let foo = b"(def (Report (Foo +infinity)))";
        use nom::Needed;
        match super::defs(CompleteByteSlice(foo)) {
            Ok((r, me)) => {
                assert_eq!(r, CompleteByteSlice(&[]));
                assert_eq!(
                    me,
                    vec![(
                        false,
                        Type::Name(String::from("Report.Foo")),
                        Type::Num(Some(u64::max_value()))
                    ),]
                );
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => panic!("{:?}", e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn reserved_names() {
        use nom::Needed;
        let foo = b"(def (__illegalname 0))";
        match super::defs(CompleteByteSlice(foo)) {
            Ok((r, me)) => panic!("Should not have succeeded: rest {:?}, result {:?}", r, me),
            Err(nom::Err::Error(_)) => (),
            Err(nom::Err::Failure(e)) => panic!("{:?}", e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn simple_event() {
        let foo = b"(when true (+ 3 4))";
        use nom::Needed;
        match super::event(CompleteByteSlice(foo)) {
            Ok((r, Ok(me))) => {
                assert_eq!(r, CompleteByteSlice(&[]));
                assert_eq!(
                    me,
                    Event {
                        flag: Expr::Atom(Prim::Bool(true)),
                        body: vec![Expr::Sexp(
                            Op::Add,
                            Box::new(Expr::Atom(Prim::Num(3))),
                            Box::new(Expr::Atom(Prim::Num(4))),
                        ),],
                    }
                );
            }
            Ok((_, Err(me))) => {
                panic!("{}", me);
            }
            Err(nom::Err::Error(_)) => (),
            Err(nom::Err::Failure(e)) => panic!("compilation error: {}", super::Error::from(e)),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn event() {
        let foo = b"
            (when (< 2 3)
                (+ 3 4)
                (* 8 7)
            )
        ";
        use nom::Needed;
        match super::event(CompleteByteSlice(foo)) {
            Ok((r, Ok(me))) => {
                assert_eq!(r, CompleteByteSlice(&[]));
                assert_eq!(
                    me,
                    Event {
                        flag: Expr::Sexp(
                            Op::Lt,
                            Box::new(Expr::Atom(Prim::Num(2))),
                            Box::new(Expr::Atom(Prim::Num(3))),
                        ),
                        body: vec![
                            Expr::Sexp(
                                Op::Add,
                                Box::new(Expr::Atom(Prim::Num(3))),
                                Box::new(Expr::Atom(Prim::Num(4))),
                            ),
                            Expr::Sexp(
                                Op::Mul,
                                Box::new(Expr::Atom(Prim::Num(8))),
                                Box::new(Expr::Atom(Prim::Num(7))),
                            ),
                        ],
                    }
                );
            }
            Ok((_, Err(me))) => {
                panic!("{}", me);
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => panic!("{:?}", e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn events() {
        let foo = b"
            (when (< 2 3)
                (+ 3 4)
                (* 8 7)
            )
            (when (< 4 5)
                (+ 4 5)
                (* 9 8)
            )
        ";
        use crate::lang::Result;
        use nom::Needed;
        match super::events(CompleteByteSlice(foo)) {
            Ok((r, me)) => {
                assert_eq!(r, CompleteByteSlice(&[]));
                let res_me: Vec<Event> = me.into_iter().collect::<Result<Vec<Event>>>().unwrap();
                assert_eq!(
                    res_me,
                    vec![
                        Event {
                            flag: Expr::Sexp(
                                Op::Lt,
                                Box::new(Expr::Atom(Prim::Num(2))),
                                Box::new(Expr::Atom(Prim::Num(3))),
                            ),
                            body: vec![
                                Expr::Sexp(
                                    Op::Add,
                                    Box::new(Expr::Atom(Prim::Num(3))),
                                    Box::new(Expr::Atom(Prim::Num(4))),
                                ),
                                Expr::Sexp(
                                    Op::Mul,
                                    Box::new(Expr::Atom(Prim::Num(8))),
                                    Box::new(Expr::Atom(Prim::Num(7))),
                                ),
                            ],
                        },
                        Event {
                            flag: Expr::Sexp(
                                Op::Lt,
                                Box::new(Expr::Atom(Prim::Num(4))),
                                Box::new(Expr::Atom(Prim::Num(5))),
                            ),
                            body: vec![
                                Expr::Sexp(
                                    Op::Add,
                                    Box::new(Expr::Atom(Prim::Num(4))),
                                    Box::new(Expr::Atom(Prim::Num(5))),
                                ),
                                Expr::Sexp(
                                    Op::Mul,
                                    Box::new(Expr::Atom(Prim::Num(9))),
                                    Box::new(Expr::Atom(Prim::Num(8))),
                                ),
                            ],
                        },
                    ],
                );
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => panic!("{:?}", e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    impl PartialEq for crate::lang::datapath::RegFile {
        fn eq(&self, other: &Self) -> bool {
            self.0.iter().zip(other.0.iter()).all(|(x, y)| x == y)
        }
    }

    impl PartialEq for Scope {
        fn eq(&self, other: &Self) -> bool {
            if self.num_perm != other.num_perm {
                return false;
            }

            self.named.eq(&other.named)
        }
    }

    #[test]
    fn combined() {
        let foo = b"
            (def (foo 0) (bar 0)) # this is a comment
            (when (> foo 0)
                (:= bar (+ bar 1)) # this is a comment
                (:= foo (* foo 2))
            )
            (when true
                (:= bar 0)
                (:= foo 0)
            )
        ";
        let (ast, sc) = Prog::new_with_scope(foo).unwrap();
        assert_eq!(sc, {
            let mut expected_scope = Scope::new();
            expected_scope.new_control(false, String::from("foo"), Type::Num(Some(0)));
            expected_scope.new_control(false, String::from("bar"), Type::Num(Some(0)));
            expected_scope
        });

        assert_eq!(
            ast,
            Prog(vec![
                Event {
                    flag: Expr::Sexp(
                        Op::Gt,
                        Box::new(Expr::Atom(Prim::Name(String::from("foo")))),
                        Box::new(Expr::Atom(Prim::Num(0))),
                    ),
                    body: vec![
                        Expr::Sexp(
                            Op::Bind,
                            Box::new(Expr::Atom(Prim::Name(String::from("bar")))),
                            Box::new(Expr::Sexp(
                                Op::Add,
                                Box::new(Expr::Atom(Prim::Name(String::from("bar")))),
                                Box::new(Expr::Atom(Prim::Num(1))),
                            )),
                        ),
                        Expr::None,
                        Expr::Sexp(
                            Op::Bind,
                            Box::new(Expr::Atom(Prim::Name(String::from("foo")))),
                            Box::new(Expr::Sexp(
                                Op::Mul,
                                Box::new(Expr::Atom(Prim::Name(String::from("foo")))),
                                Box::new(Expr::Atom(Prim::Num(2))),
                            )),
                        ),
                    ],
                },
                Event {
                    flag: Expr::Atom(Prim::Bool(true)),
                    body: vec![
                        Expr::Sexp(
                            Op::Bind,
                            Box::new(Expr::Atom(Prim::Name(String::from("bar")))),
                            Box::new(Expr::Atom(Prim::Num(0))),
                        ),
                        Expr::Sexp(
                            Op::Bind,
                            Box::new(Expr::Atom(Prim::Name(String::from("foo")))),
                            Box::new(Expr::Atom(Prim::Num(0))),
                        ),
                    ],
                },
            ]),
        );
    }

    #[test]
    fn test_complete_failure_fails() {
        let foo = b"
        (def (Report.foo 0))
        (when (> Micros 3000)
            (report
        )";

        Prog::new_with_scope(foo).unwrap_err();
    }

    #[test]
    fn test_partial_failure_fails() {
        let foo = b"
        (def (Report.foo 0))
        (when true
            (bind Report.foo 4)
            (fallthrough)
        )
        (when (> Micros 3000)
            (report
        )";

        Prog::new_with_scope(foo).unwrap_err();
    }

    #[test]
    fn test_bad_clause_fails() {
        let foo = b"
	(def
	    (Report
		(volatile acked 0)
		(volatile sacked 0)
		(volatile loss 0)
		(volatile ewmaRTT 0)
		(volatile firstRtt +infinity)
		(volatile rtt 0)
		(volatile minrtt +infinity)
	    )
	    (volatile firstAck true)
	    (volatile timeout false)
	)
	(when (&& firstAck true)
	    (:= firstAck false)
	    (:= Report.firstRtt Flow.rtt_sample_us)
	    (:= Report.ewmaRTT Flow.rtt_sample_us)
	    (fallthrough)
	)
	(when true
	    (:= Report.rtt Flow.rtt_sample_us)
	    (:= Report.acked (+ Report.acked Ack.bytes_acked))
	    (:= Report.sacked (+ Report.sacked Ack.packets_misordered))
	    (:= Report.loss Ack.lost_pkts_sample)
	    (:= Report.ewmaRTT (/ (* Report.ewmaRTT 7) 8))
	    (:= Report.ewmaRTT (+ Report.ewmaRTT (/ Flow.rtt_sample_us 8)))
	    (:= Report.minrtt (min Report.minrtt Flow.rtt_sample_us))
	    (:= timeout Flow.was_timeout)
	    (fallthrough)
	)
	(when (timeout)
	    (report)
	    (:= Micros 0)
	)
	(when (> Micros (* 2 Flow.rtt_sample_us))
	    (report)
	    (:= Micros 0)
	)
        ";

        let err = Prog::new_with_scope(foo).unwrap_err();
        println!("{:?}", err);
    }
}
