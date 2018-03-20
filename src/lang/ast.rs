use std;
use nom::IResult;
use super::{Error, Result};

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum Prim {
    Bool(bool),
    Name(String),
    Num(u64),
}

#[derive(Clone)]
#[derive(Copy)]
#[derive(Debug)]
#[derive(Eq, PartialEq)]
pub enum Op {
    Add, // (add a b) return a+b
    Bind, // (bind a b) assign variable a to value b
    Div, // (div a b) return a/b (integer division)
    Equiv, // (eq a b) return a == b
    Gt, // (> a b) return a > b
    Let, // (let (bind a b) c) with variable a bound to b, evaluate c
    Lt, // (< a b) return a < b
    Max, // (max a b) return max(a,b)
    MaxWrap, // (max a b) return max(a,b) with integer wraparound
    Min, // (min a b) return min(a,b)
    Mul, // (mul a b) return a * b
    Sub, // (sub a b) return a - b

    // SPECIAL: cannot be called by user, only generated
    Def, // top of prog: (def (Foo 0) (Bar 100000000) ...)

    // SPECIAL: cannot be bound to temp register
    If, // (if a b) if a == True, evaluate b (write return register), otherwise don't write return register

    // SPECIAL: cannot be bound to temp register
    NotIf, // (!if a b) if a == False, evaluate b (write return register), otherwise don't write return register

    // SPECIAL: reads return register
    Ewma, // (ewma a b) ret * a/10 + b * (10-a)/10.
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum Expr {
    Atom(Prim),
    Sexp(Op, Box<Expr>, Box<Expr>),
}

use std::str;
named!(
    op<Result<Op>>,
    alt!(
        alt!(tag!("+") | tag!("add"))   => { |_| Ok(Op::Add) }     |
        alt!(tag!(":=") | tag!("bind")) => { |_| Ok(Op::Bind) }    | 
        tag!("if")                      => { |_| Ok(Op::If) }      |
        alt!(tag!("/") | tag!("div"))   => { |_| Ok(Op::Div) }     | 
        alt!(tag!("==") | tag!("eq"))   => { |_| Ok(Op::Equiv) }   | 
        tag!("ewma")                    => { |_| Ok(Op::Ewma) }    | 
        alt!(tag!(">") | tag!("gt"))    => { |_| Ok(Op::Gt) }      | 
        tag!("let")                     => { |_| Ok(Op::Let) }     | 
        alt!(tag!("<") | tag!("lt"))    => { |_| Ok(Op::Lt) }      | 
        tag!("wrapped_max")             => { |_| Ok(Op::MaxWrap) } |
        tag!("max")                     => { |_| Ok(Op::Max) }     | 
        tag!("min")                     => { |_| Ok(Op::Min) }     | 
        alt!(tag!("*") | tag!("mul"))   => { |_| Ok(Op::Mul) }     |
        tag!("!if")                     => { |_| Ok(Op::NotIf) }   | 
        alt!(tag!("-") | tag!("sub"))   => { |_| Ok(Op::Sub) }     |
        atom => { |f: Result<Expr>| Err(Error::from(format!("unexpected token {:?}", f))) }
    )
);

fn check_expr(op: Op, left: Expr, right: Expr) -> Result<Expr> {
    match op {
        // let operation must have a bind clause
        Op::Let => {
            if let Expr::Sexp(Op::Bind, _, _) = left {
                Ok(Expr::Sexp(op, Box::new(left), Box::new(right)))
            } else {
                Err(Error::from(
                    format!("let requires (bind _ _) on left: {:?}", left),
                ))
            }
        }
        Op::Bind => Ok(Expr::Sexp(op, Box::new(left), Box::new(right))),
        _ => {
            match left {
                Expr::Sexp(Op::If, _, _) |
                Expr::Sexp(Op::NotIf, _, _) => Err(Error::from(
                    format!("conditionals cannot be bound to temp registers: {:?}", left),
                )),
                _ => Ok(Expr::Sexp(op, Box::new(left), Box::new(right))),
            }
        }
    }
}

use nom::multispace;
named!(
    sexp<Result<Expr>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            opt!(multispace) >>
            first: op >>
            opt!(multispace) >>
            second: expr >>
            opt!(multispace) >>
            third: expr >>
            (first.and_then(
                |opr| second.and_then(
                |left| third.and_then(
                |right| check_expr(opr, left, right)
            ))))
        ),
        tag!(")")
    ))
);

use nom::digit;
use std::str::FromStr;
named!(
    num<u64>,
    map_res!(
        map_res!(digit, str::from_utf8),
        FromStr::from_str
    )
);

use nom::is_alphanumeric;
named!(
    name<&[u8]>,
    take_while1!(|u: u8| is_alphanumeric(u) || u == b'.' || u == b'_')
);

use nom::rest;
named!(
    atom<Result<Expr>>,
    ws!(do_parse!(
        val: alt_complete!(
            tag!("true")  => { |_| Ok(Prim::Bool(true)) }  |
            tag!("false") => { |_| Ok(Prim::Bool(false)) } |
            tag!("+infinity") => { |_| Ok(Prim::Num(u64::max_value())) } |
            num => { |n: u64| Ok(Prim::Num(n)) } |
            name => { |n: &[u8]| match String::from_utf8(n.to_vec()) {
                Ok(s) => Ok(Prim::Name(s)),
                Err(e) => Err(Error::from(e)),
            } } |
            rest => { 
                |f: &[u8]| {
                    let rest = std::str::from_utf8(f).unwrap();
                    Err(Error::from(format!("unexpected: {:?}", rest))) 
                }
            }
        ) >>
        (val.and_then(|t| Ok(Expr::Atom(t))))
    ))
);

named!(
    expr<Result<Expr>>,
    alt_complete!(sexp | atom)
);

named!(
    exprs<Vec<Result<Expr>>>,
    many1!(expr)
);

impl Expr {
    // TODO make return Iter
    pub(crate) fn new(src: &[u8]) -> Result<Vec<Self>> {
        use nom::Needed;
        match exprs(src) {
            IResult::Done(_, me) => {
                let mut prog = vec![];
                for exp in me {
                    prog.push(exp?);
                }

                Ok(prog)
            }
            IResult::Error(e) => Err(Error::from(e)),
            IResult::Incomplete(Needed::Unknown) => Err(Error::from(String::from("need more src"))),
            IResult::Incomplete(Needed::Size(s)) => Err(
                Error::from(format!("need {} more bytes", s)),
            ),
        }
    }
}

use super::datapath::{Scope, Type, check_atom_type};
/// a Prog is multiple Expr in sequence.
/// Scope cascades through the Expr:
/// Expr with `Type::Name` will in scope for successive Expr
/// Other Expr will not be evaluated.
#[derive(Debug)]
pub struct Prog(pub Vec<Expr>);

/// Declare a state variable and provide an initial value
/// Optionally use Report. as a variable prefix, to match the fold function body:
/// (Foo 0) (Report.this_is_allowed 14) (Bar true) 
/// See also `portus::lang::ast::tests::defs1()`.
named!(
    decl<(Type, Type)>,
    delimited!(
        tag!("("),
        tuple!(
			map_res!(do_parse!(
				opt!(tag!("Report.")) >>
				n: name >>
				(n)
			), |b| str::from_utf8(b).and_then(|i| Ok(Type::Name(String::from(i))))),
            map_res!(atom, |a: Result<Expr>| a.and_then(|i| check_atom_type(&i)))
        ),
        tag!(")")
    )
);
/// a Prog has special syntax *at the beginning* to declare the Report state variables.
/// (def (decl) ...)
named!(
    defs<Vec<(Type, Type)>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            tag!("def") >> 
            defs : many1!(decl) >>
            (
                defs.into_iter().map(|(name, init_val)| {
                    match init_val {
                        x@ Type::Num(_) | x@ Type::Bool(_) => (name, x),
                        _ => (name, Type::None)
                    } 
                }).collect()
            )
        ),
        tag!(")")
    ))
);

impl Prog {
    pub(crate) fn new_with_scope(source: &[u8]) -> Result<(Self, Scope)> {
        let mut scope = Scope::new();
        use nom::{IResult, Needed};
        let rest = match defs(source) {
            IResult::Done(rest, flow_state) => {
                flow_state
                    .into_iter()
                    .map(|(var, typ)| match var {
                        Type::Name(v) => (format!("Report.{}", v), typ),
                        _ => unreachable!(),
                    })
                    .for_each(|(var, typ)| { scope.new_perm(var, typ); });

                Ok(rest)
            }
            IResult::Error(e) => Err(Error::from(e)),
            IResult::Incomplete(Needed::Unknown) => Err(Error::from(String::from("need more src"))),
            IResult::Incomplete(Needed::Size(s)) => Err(
                Error::from(format!("need {} more bytes", s)),
            ),
        }?;

        // TODO make Expr::new return Iter, make self wrap an iter also
        Ok((Prog(Expr::new(rest)?), scope))
    }
}

#[cfg(test)]
mod tests {
    use super::{Expr, Op, Prim};

    #[test]
    fn atom() {
        let foo = b"1";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);

        let foo = b"1 ";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);

        let foo = b"+";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }

        let foo = b"true";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Bool(true))]);

        let foo = b"false";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Bool(false))]);

        let foo = b"x";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("x")))]);

        let foo = b"acbdefg";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("acbdefg")))]);
    }

    #[test]
    fn simple() {
        let foo = b"(+ 10 20)";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![
                Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Atom(Prim::Num(10))),
                    Box::new(Expr::Atom(Prim::Num(20)))
                ),
            ]
        );

        let foo = b"(blah 10 20)";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }
    }


    #[test]
    fn maxtest() {
        let foo = b"(wrapped_max 10 20)";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![
                Expr::Sexp(
                    Op::MaxWrap,
                    Box::new(Expr::Atom(Prim::Num(10))),
                    Box::new(Expr::Atom(Prim::Num(20)))
                ),
            ]
        );
    }

    #[test]
    fn tree() {
        let foo = b"(+ (+ 7 3) (+ 4 6))";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![
                Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(7))),
                        Box::new(Expr::Atom(Prim::Num(3))),
                    )),
                    Box::new(Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(4))),
                        Box::new(Expr::Atom(Prim::Num(6))),
                    ))
                ),
            ]
        );

        let foo = b"(+ (- 17 7) (+ 4 (- 26 20)))";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![
                Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Sexp(
                        Op::Sub,
                        Box::new(Expr::Atom(Prim::Num(17))),
                        Box::new(Expr::Atom(Prim::Num(7))),
                    )),
                    Box::new(Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(4))),
                        Box::new(Expr::Sexp(
                            Op::Sub,
                            Box::new(Expr::Atom(Prim::Num(26))),
                            Box::new(Expr::Atom(Prim::Num(20))),
                        )),
                    ))
                ),
            ]
        );
    }

    #[test]
    fn whitespace() {
        let foo = b"
            (
                +
                (
                    -
                    17
                    7
                )
                (
                    +
                    4
                    (
                        -
                        26
                        20
                    )
                )
            )";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![
                Expr::Sexp(
                    Op::Add,
                    Box::new(Expr::Sexp(
                        Op::Sub,
                        Box::new(Expr::Atom(Prim::Num(17))),
                        Box::new(Expr::Atom(Prim::Num(7))),
                    )),
                    Box::new(Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(4))),
                        Box::new(Expr::Sexp(
                            Op::Sub,
                            Box::new(Expr::Atom(Prim::Num(26))),
                            Box::new(Expr::Atom(Prim::Num(20))),
                        )),
                    ))
                ),
            ]
        );
    }

    use lang::datapath::Type;
    #[test]
    fn defs() {
        let foo = b"(def (Foo 0) (Bar 0) (Baz 0))";
        use nom::{IResult, Needed};
        match super::defs(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                assert_eq!(
                me,
                vec![
                    (Type::Name(String::from("Foo")), Type::Num(Some(0))),
                    (Type::Name(String::from("Bar")), Type::Num(Some(0))),
                    (Type::Name(String::from("Baz")), Type::Num(Some(0))),
                ]
            );
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn defs1() {
        let foo = b"(def (Foo 0) (Report.this_is_allowed 14) (Bar true))";
        use nom::{IResult, Needed};
        match super::defs(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                assert_eq!(
                me,
                vec![
                    (Type::Name(String::from("Foo")), Type::Num(Some(0))),
                    (Type::Name(String::from("this_is_allowed")), Type::Num(Some(14))),
                    (Type::Name(String::from("Bar")), Type::Bool(Some(true))),
                ]
            );
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn infinity() {
        let foo = b"(def (Foo +infinity))";
        use nom::{IResult, Needed};
        match super::defs(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                assert_eq!(
                me,
                vec![
                    (Type::Name(String::from("Foo")), Type::Num(Some(u64::max_value()))),
                ]
            );
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }
}
