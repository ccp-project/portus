use super::{Error, Result};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::{
        complete::{digit1, multispace0},
        is_alphanumeric,
    },
    combinator::{map, map_res, opt},
    sequence::{delimited, tuple},
};

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

pub fn op(input: &str) -> nom::IResult<&str, Op> {
    alt((
        map(alt((tag("+"), tag("add"))), |_| Op::Add),
        map(alt((tag("&&"), tag("and"))), |_| Op::And),
        map(alt((tag(":="), tag("bind"))), |_| Op::Bind),
        map(tag("if"), |_| Op::If),
        map(alt((tag("/"), tag("div"))), |_| Op::Div),
        map(alt((tag("=="), tag("eq"))), |_| Op::Equiv),
        map(tag("ewma"), |_| Op::Ewma),
        map(alt((tag(">"), tag("gt"))), |_| Op::Gt),
        map(alt((tag("<"), tag("lt"))), |_| Op::Lt),
        map(tag("wrapped_max"), |_| Op::MaxWrap),
        map(tag("max"), |_| Op::Max),
        map(tag("min"), |_| Op::Min),
        map(alt((tag("*"), tag("mul"))), |_| Op::Mul),
        map(alt((tag("||"), tag("or"))), |_| Op::Or),
        map(tag("!if"), |_| Op::NotIf),
        map(alt((tag("-"), tag("sub"))), |_| Op::Sub),
    ))(input)
}

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

pub fn sexp(input: &str) -> nom::IResult<&str, Expr> {
    delimited(
        tag("("),
        delimited(
            multispace0,
            map_res(
                tuple((op, opt(multispace0), expr, opt(multispace0), expr)),
                |(op, _, left, _, right)| check_expr(op, left, right),
            ),
            multispace0,
        ),
        tag(")"),
    )(input)
}

pub fn num(input: &str) -> nom::IResult<&str, u64> {
    map_res(digit1, |s: &str| s.parse())(input)
}

pub fn name(input: &str) -> nom::IResult<&str, String> {
    map_res(
        take_while1(|u: char| is_alphanumeric(u as _) || u == '.' || u == '_'),
        |s: &str| {
            if s.starts_with("__") {
                Err(Error::from(format!(
                    "Names beginning with \"__\" are reserved for internal use: {:?}",
                    s
                )))
            } else {
                Ok(s.to_string())
            }
        },
    )(input)
}

pub fn atom(input: &str) -> nom::IResult<&str, Expr> {
    map(
        alt((
            map(tag("true"), |_| Prim::Bool(true)),
            map(tag("false"), |_| Prim::Bool(false)),
            map(tag("+infinity"), |_| Prim::Num(u64::MAX)),
            map(num, Prim::Num),
            map(name, Prim::Name),
        )),
        Expr::Atom,
    )(input)
}

pub fn command(input: &str) -> nom::IResult<&str, Expr> {
    map(
        delimited(
            tag("("),
            delimited(
                multispace0,
                alt((
                    map(tag("fallthrough"), |_| Command::Fallthrough),
                    map(tag("report"), |_| Command::Report),
                )),
                multispace0,
            ),
            tag(")"),
        ),
        Expr::Cmd,
    )(input)
}

pub fn comment(input: &str) -> nom::IResult<&str, Expr> {
    map(tuple((tag("#"), take_until("\n"))), |_| Expr::None)(input)
}

pub fn expr(input: &str) -> nom::IResult<&str, Expr> {
    delimited(
        multispace0,
        alt((comment, sexp, command, atom)),
        multispace0,
    )(input)
}

pub fn exprs(input: &str) -> nom::IResult<&str, Vec<Expr>> {
    nom::multi::many1(expr)(input)
}

impl Expr {
    // TODO make return Iter
    pub fn new(src: &str) -> Result<Vec<Self>> {
        match exprs(src) {
            Ok((rest, _)) if !rest.is_empty() => {
                Err(Error::from(format!("compile error: \"{}\"", rest)))
            }
            Ok((_, me)) => Ok(me
                .into_iter()
                .filter(|e| !matches!(e, Expr::None))
                .collect()),
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => Err(Error::from(e)),
            Err(nom::Err::Incomplete(nom::Needed::Unknown)) => Err(Error::from("need more src")),
            Err(nom::Err::Incomplete(nom::Needed::Size(s))) => {
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
            Expr::Sexp(_, ref mut left, ref mut right) => {
                left.desugar();
                right.desugar();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Expr, Op, Prim};

    #[test]
    fn atom_0() {
        use super::name;
        let foo = "foo";
        let er = name(foo);
        println!("{:?}", er.expect("parse single atom"));
    }

    #[test]
    fn atom_1() {
        let foo = "1";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);
    }

    #[test]
    fn atom_2() {
        let foo = "1 ";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Num(1))]);
    }

    #[test]
    fn atom_3() {
        let foo = "+";
        let er = Expr::new(foo);
        if let Ok(e) = er {
            panic!("false ok: {:?}", e);
        }
    }

    #[test]
    fn atom_4() {
        let foo = "true";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Bool(true))]);
    }

    #[test]
    fn atom_5() {
        let foo = "false";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Bool(false))]);
    }

    #[test]
    fn atom_6() {
        let foo = "x";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("x")))]);
    }

    #[test]
    fn atom_7() {
        let foo = "acbdefg";
        let er = Expr::new(foo);
        let e = er.unwrap();
        assert_eq!(e, vec![Expr::Atom(Prim::Name(String::from("acbdefg")))]);
    }

    #[test]
    fn atom_8() {
        let foo = "blah 10 20";
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
        let foo = "(+ 10 20)";
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

        let foo = "(blah 10 20)";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }

        let foo = "(blah 10 20";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }
    }

    #[test]
    fn bool_ops() {
        let foo = "(&& true false)";
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

        let foo = "(|| 10 20)";
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
        let foo = "(+ 10 20))";
        use super::exprs;
        use nom::Needed;
        match exprs(foo) {
            Ok((r, me)) => {
                assert_eq!(r, ")");
                assert_eq!(
                    me.into_iter().collect::<Vec<Expr>>(),
                    vec![Expr::Sexp(
                        Op::Add,
                        Box::new(Expr::Atom(Prim::Num(10))),
                        Box::new(Expr::Atom(Prim::Num(20)))
                    ),],
                );
            }
            Err(nom::Err::Error(e)) | Err(nom::Err::Failure(e)) => panic!("{:?}", e),
            Err(nom::Err::Incomplete(Needed::Unknown)) => panic!("incomplete"),
            Err(nom::Err::Incomplete(Needed::Size(s))) => panic!("need {} more bytes", s),
        }
    }

    #[test]
    fn maxtest() {
        let foo = "(wrapped_max 10 20)";
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
        let foo = "(+ (+ 7 3) (+ 4 6))";
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

        let foo = "(+ (- 17 7) (+ 4 (- 26 20)))";
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
        let foo = "
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
        let foo = "
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
    fn partial() {
        let foo = "
            (:= foo 1)
            (report
        ";

        Expr::new(foo).unwrap_err();
    }

    #[test]
    fn comments() {
        let foo = "
            # such comments
            (report) # very descriptive # wow (+ 2 3)
            # much documentation
        ";

        let e = Expr::new(foo).unwrap();
        assert_eq!(e, vec![Expr::Cmd(Command::Report),]);
    }

    #[test]
    fn old_syntax() {
        let foo = "(reset)";
        let er = Expr::new(foo);
        match er {
            Ok(e) => panic!("false ok: {:?}", e),
            Err(_) => (),
        }
    }
}
