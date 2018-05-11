use super::{Error, Result};
use super::ast::{atom, expr, Expr, exprs, name};
use super::datapath::{Scope, Type, check_atom_type};

/// An `Event` is a condition expression and a sequence of execution expressions.
/// If the condition expression evaluates to `true`, the execution expressions are
/// evaluated.
#[derive(Debug, PartialEq)]
pub struct Event {
    pub flag: Expr,
    pub body: Vec<Expr>,
}

/// a `Prog` is multiple `Event`
/// Scope cascades through the `Expr`:
/// Expr with `Type::Name` will in scope for successive `Expr`
/// Other `Expr` will not be evaluated.
#[derive(Debug, PartialEq)]
pub struct Prog(pub Vec<Event>);

// ------------------------------------------
// (def (decl)...) grammar
// ------------------------------------------

/// Declare a state variable and provide an initial value
/// Optionally declare the variable "volatile", meaning it gets reset on "(report)"
named!(
    decl<(bool, Type, Type)>,
    ws!(delimited!(
        tag!("("),
        tuple!(
            map!(opt!(tag!("volatile")), |v: Option<&[u8]>| v.is_some()),
            map!(name, |n: String| Type::Name(n)),
            map_res!(atom, |a: Result<Expr>| a.and_then(|i| check_atom_type(&i)))
        ),
        tag!(")")
    ))
);
named!(
    report_struct<Vec<(bool, Type, Type)>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            tag!("Report") >>
            d: many1!(decl) >> 
            (d)
        ),
        tag!(")")
    ))
);

/// a Prog has special syntax *at the beginning* to declare variables.
/// (def (decl) ...)
named!(
    defs<Vec<(bool, Type, Type)>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            tag!("def") >> 
            defs1 : many0!(decl) >>
            reports : opt!(report_struct) >>
            defs2 :  many0!(decl) >>
            (
                reports.into_iter().flat_map(|v| v.into_iter())
                    .filter_map(|(is_volatile, name, init_val)| {
                        match name {
                            Type::Name(name) => Some(Type::Name(format!("Report.{}", name))),
                            _ => None
                        }.map(|full_name| 
                            match init_val {
                                x@ Type::Num(_) | x@ Type::Bool(_) => (is_volatile, full_name, x),
                                _ => (is_volatile, full_name, Type::None)
                            }
                        )
                    })
                    .chain(
                        defs1.into_iter()
                            .chain(defs2)
                            .map(|(is_volatile, name, init_val)| {
                            match init_val {
                                x@ Type::Num(_) | x@ Type::Bool(_) => (is_volatile, name, x),
                                _ => (is_volatile, name, Type::None)
                            } 
                        })
                    ).collect()
            )
        ),
        tag!(")")
    ))
);

// ------------------------------------------
// (when (bool expr) (body)...) grammar
// ------------------------------------------

/// (when (single expr) (expr)...)
named!(
    event<Result<Event>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            tag!("when") >>
            c : expr >>
            body : exprs >>
            (
                c.and_then(|cond| {
                    let exps: Result<Vec<Expr>> = body.into_iter().collect();
                    Ok(Event{
                        flag: cond,
                        body: exps?,
                    })
                })
            )
        ),
        tag!(")")
    ))
);
named!(
    events<Vec<Result<Event>>>,
    many1!(event)
);

impl Prog {
    pub fn new_with_scope(source: &[u8]) -> Result<(Self, Scope)> {
        let mut scope = Scope::new();
        use nom::{IResult, Needed};
        let body = match defs(source) {
            IResult::Done(rest, flow_state) => {
                let (reports, controls): (Vec<(bool, String, Type)>, Vec<(bool, String, Type)>) = flow_state
                    .into_iter()
                    .map(|(is_volatile, var, typ)| match var {
                        Type::Name(v) => (is_volatile, v, typ),
                        _ => unreachable!(),
                    })
                    .partition(|&(_, ref var, _)| var.starts_with("Report."));

                for (is_volatile, var, typ) in reports {
                    scope.new_report(is_volatile, var, typ); 
                }

                for (_is_volatile, var, typ) in controls {
                    scope.new_control(var, typ);
                }

                Ok(rest)
            }
            IResult::Error(e) => Err(Error::from(e)),
            IResult::Incomplete(Needed::Unknown) => Err(Error::from(String::from("need more src"))),
            IResult::Incomplete(Needed::Size(s)) => Err(
                Error::from(format!("need {} more bytes", s)),
            ),
        }?;

        let evs = match events(body) {
            IResult::Done(_, me) => me.into_iter().collect(),
            IResult::Error(e) => Err(Error::from(e)),
            IResult::Incomplete(Needed::Unknown) => Err(Error::from("need more src")),
            IResult::Incomplete(Needed::Size(s)) => Err(
                Error::from(format!("need {} more bytes", s)),
            ),
        }?;

        let mut p = Prog(evs);
        p.desugar();

        // TODO make Expr::new return Iter, make self wrap an iter also
        Ok((p, scope))
    }
    
    fn desugar(&mut self) {
        self.0.iter_mut()
            .for_each(|v| v.body.iter_mut()
            .for_each(|e| e.desugar()));
    }
}

#[cfg(test)]
mod tests {
    use lang::ast::{Expr, Op, Prim};
    use lang::prog::{Event,Prog};
    use lang::datapath::{Scope, Type};

    #[test]
    fn defs() {
        let foo = b"(def (Bar 0) (Report (Foo 0) (volatile Baz 0)) (Qux 0))";
        use nom::{IResult, Needed};
        match super::defs(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                assert_eq!(
                me,
                vec![
                    (false, Type::Name(String::from("Report.Foo")), Type::Num(Some(0))),
                    (true, Type::Name(String::from("Report.Baz")), Type::Num(Some(0))),
                    (false, Type::Name(String::from("Bar")), Type::Num(Some(0))),
                    (false, Type::Name(String::from("Qux")), Type::Num(Some(0))),
                ]
            );
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn def_infinity() {
        let foo = b"(def (Report (Foo +infinity)))";
        use nom::{IResult, Needed};
        match super::defs(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                assert_eq!(
                    me,
                    vec![
                        (false, Type::Name(String::from("Report.Foo")), Type::Num(Some(u64::max_value()))),
                    ]
                );
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn reserved_names() {
        use nom::{IResult, Needed};
        let foo = b"(def (__illegalname 0))";
        match super::defs(foo) {
            IResult::Done(r, me) => panic!("Should not have succeeded: rest {:?}, result {:?}", r, me),
            IResult::Error(_) => (),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn simple_event() {
        let foo = b"(when true (+ 3 4))";
        use nom::{IResult, Needed};
        match super::event(foo) {
            IResult::Done(r, Ok(me)) => {
                assert_eq!(r, &[]);
                assert_eq!(
                    me,
                    Event{
                        flag: Expr::Atom(Prim::Bool(true)),
                        body: vec![
                            Expr::Sexp(
                                Op::Add,
                                Box::new(Expr::Atom(Prim::Num(3))),
                                Box::new(Expr::Atom(Prim::Num(4))),
                            ),
                        ],
                    }
                );
            }
            IResult::Done(_, Err(me)) => {
                panic!(me);
            }
            IResult::Error(e) => panic!("compilation error: {}", e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
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
        use nom::{IResult, Needed};
        match super::event(foo) {
            IResult::Done(r, Ok(me)) => {
                assert_eq!(r, &[]);
                assert_eq!(
                    me,
                    Event{
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
            IResult::Done(_, Err(me)) => {
                panic!(me);
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
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
        use nom::{IResult, Needed};
        use ::lang::Result;
        match super::events(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                let res_me: Vec<Event> = me.into_iter().collect::<Result<Vec<Event>>>().unwrap();
                assert_eq!(
                    res_me,
                    vec![
                        Event{
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
                        Event{
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
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }

    impl PartialEq for ::lang::datapath::RegFile {
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
            (def (foo 0) (bar 0))
            (when (> foo 0)
                (:= bar (+ bar 1))
                (:= foo (* foo 2))
            )
            (when true
                (:= bar 0)
                (:= foo 0)
            )
        ";
        let (ast, sc) = Prog::new_with_scope(foo).unwrap();
        assert_eq!(
            sc,
            {
                let mut expected_scope = Scope::new();
                expected_scope.new_control(String::from("foo"), Type::Num(Some(0)));
                expected_scope.new_control(String::from("bar"), Type::Num(Some(0)));
                expected_scope
            }
        );

        assert_eq!(
            ast,
            Prog(vec![
                Event{
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
                Event{
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
}
