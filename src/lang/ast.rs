use super::{Error, Result};
use nom;

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Bool(bool),
    Name(String),
    Num(u64),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Op {
    Add,     // (add a b) return a+b
    And,     // (and a b) return a && b
    Bind,    // (bind a b) assign variable a to value b
    Div,     // (div a b) return a/b (integer division)
    Equiv,   // (eq a b) return a == b
    Gt,      // (> a b) return a > b
    Lt,      // (< a b) return a < b
    Max,     // (max a b) return max(a,b)
    MaxWrap, // (max a b) return max(a,b) with integer wraparound
    Min,     // (min a b) return min(a,b)
    Mul,     // (mul a b) return a * b
    Or,      // (or a b) return a || b
    Sub,     // (sub a b) return a - b

    // SPECIAL: cannot be called by user, only generated
    Def, // top of prog: (def (Foo 0) (Bar 100000000) ...)

    // SPECIAL: cannot be bound to temp register
    If, // (if a b) if a == True, evaluate b (write return register), otherwise don't write return register
    NotIf, // (!if a b) if a == False, evaluate b (write return register), otherwise don't write return register

    // SPECIAL: reads return register
    Ewma, // (ewma a b) ret * a/10 + b * (10-a)/10.
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Command {
    Fallthrough, // Continue and evaluate the next `when` clause. desugars to `(:= shouldContinue true)`
    Report,      // Send a report. desugars to `(bind shouldReport true)`
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Atom(Prim),
    Cmd(Command),
    Sexp(Op, Box<Expr>, Box<Expr>),
    None,
}

use std::str;
named_complete!(
    op<Result<Op>>,
    alt!(
        alt!(tag!("+") | tag!("add"))   => { |_| Ok(Op::Add) }     |
        alt!(tag!("&&") | tag!("and"))  => { |_| Ok(Op::And) }     |
        alt!(tag!(":=") | tag!("bind")) => { |_| Ok(Op::Bind) }    |
        tag!("if")                      => { |_| Ok(Op::If) }      |
        alt!(tag!("/") | tag!("div"))   => { |_| Ok(Op::Div) }     |
        alt!(tag!("==") | tag!("eq"))   => { |_| Ok(Op::Equiv) }   |
        tag!("ewma")                    => { |_| Ok(Op::Ewma) }    |
        alt!(tag!(">") | tag!("gt"))    => { |_| Ok(Op::Gt) }      |
        alt!(tag!("<") | tag!("lt"))    => { |_| Ok(Op::Lt) }      |
        tag!("wrapped_max")             => { |_| Ok(Op::MaxWrap) } |
        tag!("max")                     => { |_| Ok(Op::Max) }     |
        tag!("min")                     => { |_| Ok(Op::Min) }     |
        alt!(tag!("*") | tag!("mul"))   => { |_| Ok(Op::Mul) }     |
        alt!(tag!("||") | tag!("or"))   => { |_| Ok(Op::Or) }      |
        tag!("!if")                     => { |_| Ok(Op::NotIf) }   |
        alt!(tag!("-") | tag!("sub"))   => { |_| Ok(Op::Sub) }     |
        atom => { |f: Result<Expr>| Err(Error::from(format!("unexpected token {:?}", f))) }
    )
);

fn check_expr(op: Op, left: Expr, right: Expr) -> Result<Expr> {
    match op {
        Op::Bind => Ok(Expr::Sexp(op, Box::new(left), Box::new(right))),
        _ => match (&left, &right) {
            (&Expr::Sexp(Op::If, _, _), _) | (&Expr::Sexp(Op::NotIf, _, _), _) => {
                Err(Error::from(format!(
                    "Conditional cannot be bound to temp register: {:?}",
                    left.clone()
                )))
            }
            _ => Ok(Expr::Sexp(op, Box::new(left), Box::new(right))),
        },
    }
}

use nom::multispace;
named_complete!(
    sexp<Result<Expr>>,
    ws!(delimited!(
        tag!("("),
        do_parse!(
            first: op
                >> opt!(multispace)
                >> second: expr
                >> opt!(multispace)
                >> third: expr
                >> (first.and_then(|opr| second
                    .and_then(|left| third.and_then(|right| check_expr(opr, left, right)))))
        ),
        tag!(")")
    ))
);

use nom::digit;
use nom::types::CompleteByteSlice;
use std::str::FromStr;
named_complete!(
    pub num<u64>,
    map_res!(
        digit,
        |d: CompleteByteSlice| {
            let st = str::from_utf8(d.0)?;
            FromStr::from_str(st).map_err(Error::from)
        }
    )
);

use nom::is_alphanumeric;
named_complete!(
    pub name<String>,
    map_res!(
        take_while1!(|u: u8| is_alphanumeric(u) || u == b'.' || u == b'_'),
        |n: CompleteByteSlice| str::from_utf8(n.0).map_err(Error::from).and_then(|s|
            if s.starts_with("__") {
                Err(Error::from(
                    format!("Names beginning with \"__\" are reserved for internal use: {:?}", s),
                ))
            } else {
                Ok(String::from(s))
            }
        )
    )
);

named_complete!(
    pub atom<Result<Expr>>,
    ws!(do_parse!(
        val: alt!(
            tag!("true")  => { |_| Ok(Prim::Bool(true)) }  |
            tag!("false") => { |_| Ok(Prim::Bool(false)) } |
            tag!("+infinity") => { |_| Ok(Prim::Num(u64::max_value())) } |
            num => { |n: u64| Ok(Prim::Num(n)) } |
            name => { |n: String| Ok(Prim::Name(n)) }
        ) >>
        (val.and_then(|t| Ok(Expr::Atom(t))))
    ))
);

named_complete!(
    command<Result<Expr>>,
    ws!(delimited!(
        tag!("("),
        map!(
            alt!(
                tag!("fallthrough") => { |_| Command::Fallthrough } |
                tag!("report")      => { |_| Command::Report      }
            ),
            |c| Ok(Expr::Cmd(c))
        ),
        tag!(")")
    ))
);

named_complete!(
    pub comment<Result<Expr>>,
    ws!(do_parse!(
        tag!("#") >>
        take_until!("\n") >>
        (Ok(Expr::None))
    ))
);

named_complete!(
    pub expr<Result<Expr>>,
    alt_complete!(comment | sexp | command | atom)
);

named_complete!(
    pub exprs<Vec<Result<Expr>>>,
    many1!(expr)
);

impl Expr {
    // TODO make return Iter
    pub fn new(src: &[u8]) -> Result<Vec<Self>> {
        use nom::Needed;
        match exprs(CompleteByteSlice(src)) {
            Ok((_, me)) => me
                .into_iter()
                .filter(|e| match e {
                    Ok(Expr::None) => false,
                    _ => true,
                })
                .collect(),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Err(Error::from(e)),
            Err(nom::Err::Incomplete(Needed::Unknown)) => Err(Error::from("need more src")),
            Err(nom::Err::Incomplete(Needed::Size(s))) => {
                Err(Error::from(format!("need {} more bytes", s)))
            }
        }
    }

    pub fn desugar(&mut self) {
        match *self {
            Expr::Cmd(Command::Fallthrough) => {
                *self = Expr::Sexp(
                    Op::Bind,
                    Box::new(Expr::Atom(Prim::Name(String::from("__shouldContinue")))),
                    Box::new(Expr::Atom(Prim::Bool(true))),
                )
            }
            Expr::Cmd(Command::Report) => {
                *self = Expr::Sexp(
                    Op::Bind,
                    Box::new(Expr::Atom(Prim::Name(String::from("__shouldReport")))),
                    Box::new(Expr::Atom(Prim::Bool(true))),
                )
            }
            Expr::None => {}
            Expr::Atom(_) => {}
            Expr::Sexp(_, box ref mut left, box ref mut right) => {
                left.desugar();
                right.desugar();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Expr, Op, Prim};
    use nom::types::CompleteByteSlice;

    #[test]
    fn atom_0() {
        use super::name;
        let foo = b"foo";
        let er = name(CompleteByteSlice(foo));
        println!("{:?}", er.expect("parse single atom"));
    }

    #[test]
    fn atom_1() {
        let foo = b"1";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);
    }

    #[test]
    fn atom_2() {
        let foo = b"1 ";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);
    }

    #[test]
    fn atom_3() {
        let foo = b"+";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }
    }

    #[test]
    fn atom_4() {
        let foo = b"true";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Bool(true))]);
    }

    #[test]
    fn atom_5() {
        let foo = b"false";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Bool(false))]);
    }

    #[test]
    fn atom_6() {
        let foo = b"x";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("x")))]);
    }

    #[test]
    fn atom_7() {
        let foo = b"acbdefg";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("acbdefg")))]);
    }

    #[test]
    fn atom_8() {
        let foo = b"blah 10 20";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![
                Expr::Atom(Prim::Name(String::from("blah"))),
                Expr::Atom(Prim::Num(10)),
                Expr::Atom(Prim::Num(20)),
            ]
        );
    }

    #[test]
    fn simple_exprs() {
        let foo = b"(+ 10 20)";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![Expr::Sexp(
                Op::Add,
                Box::new(Expr::Atom(Prim::Num(10))),
                Box::new(Expr::Atom(Prim::Num(20)))
            ),]
        );

        let foo = b"(blah 10 20)";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }

        let foo = b"(blah 10 20";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }
    }

    #[test]
    fn bool_ops() {
        let foo = b"(&& true false)";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![Expr::Sexp(
                Op::And,
                Box::new(Expr::Atom(Prim::Bool(true))),
                Box::new(Expr::Atom(Prim::Bool(false))),
            ),]
        );

        let foo = b"(|| 10 20)";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![Expr::Sexp(
                Op::Or,
                Box::new(Expr::Atom(Prim::Num(10))),
                Box::new(Expr::Atom(Prim::Num(20)))
            ),]
        );
    }

    #[test]
    fn expr_leftover() {
        use nom;
        let foo = b"(+ 10 20))";
        use super::exprs;
        use crate::lang::Result;
        use nom::Needed;
        match exprs(CompleteByteSlice(foo)) {
            Ok((r, me)) => {
                assert_eq!(r, CompleteByteSlice(b")"));
                assert_eq!(
                    me.into_iter().collect::<Result<Vec<Expr>>>().unwrap(),
                    vec![Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(10))),
                        Box::new(Expr::Atom(Prim::Num(20)))
                    ),],
                );
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => panic!(e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn maxtest() {
        let foo = b"(wrapped_max 10 20)";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![Expr::Sexp(
                Op::MaxWrap,
                Box::new(Expr::Atom(Prim::Num(10))),
                Box::new(Expr::Atom(Prim::Num(20)))
            ),]
        );
    }

    #[test]
    fn tree() {
        let foo = b"(+ (+ 7 3) (+ 4 6))";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![Expr::Sexp(
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
            ),]
        );

        let foo = b"(+ (- 17 7) (+ 4 (- 26 20)))";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(
            e,
            vec![Expr::Sexp(
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
            ),]
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
            vec![Expr::Sexp(
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
            ),]
        );
    }

    #[test]
    fn commands() {
        let foo = b"
            (report)
            (fallthrough)
        ";

        let e = Expr::new(foo).unwrap();
        assert_eq!(
            e,
            vec![Expr::Cmd(Command::Report), Expr::Cmd(Command::Fallthrough),]
        );
    }

    #[test]
    fn comments() {
        let foo = b"
            # such comments
            (report) # very descriptive # wow (+ 2 3)
            # much documentation
        ";

        let e = Expr::new(foo).unwrap();
        assert_eq!(e, vec![Expr::Cmd(Command::Report),]);
    }

    #[test]
    fn old_syntax() {
        let foo = b"(reset)";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }
    }
}
