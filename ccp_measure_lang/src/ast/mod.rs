use nom::IResult;
use super::{Error, Result};

#[derive(Debug)]
#[derive(PartialEq)]
pub enum Prim {
    Bool(bool),
    Name(String),
    Num(u32),
    None,
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq, Eq, Hash)]
pub enum Type {
    Bool,
    Name(String),
    Num,
    None,
    Var(String),
}

#[derive(Debug)]
#[derive(PartialEq)]
pub enum Op {
    Add,
    Beq,
    Bind,
    Bne,
    Div,
    Equiv,
    Gt,
    Let,
    Lt,
    Mul,
    Sub,
    Then,
}

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
        alt!(tag!("+") | tag!("add"))   => { |_| Ok(Op::Add) }   |
        tag!("if")                      => { |_| Ok(Op::Beq) }   | 
        alt!(tag!(":=") | tag!("bind")) => { |_| Ok(Op::Bind) }  | 
        tag!("!if")                     => { |_| Ok(Op::Bne) }   | 
        alt!(tag!("/") | tag!("div"))   => { |_| Ok(Op::Div) }   | 
        alt!(tag!("==") | tag!("eq"))   => { |_| Ok(Op::Equiv) } | 
        alt!(tag!(">") | tag!("gt"))    => { |_| Ok(Op::Gt) }    | 
        tag!("let")                     => { |_| Ok(Op::Let) }   | 
        alt!(tag!("<") | tag!("lt"))    => { |_| Ok(Op::Lt) }    | 
        alt!(tag!("*") | tag!("mul"))   => { |_| Ok(Op::Mul) }   |
        alt!(tag!("-") | tag!("sub"))   => { |_| Ok(Op::Sub) }   |
        tag!("then")                    => { |_| Ok(Op::Then) }  | 
        atom => { |f: Result<Expr>| Err(Error::from(format!("unexpected token {:?}", f))) }
    )
);

fn check_expr(op: Op, left: Expr, right: Expr) -> Result<Expr> {
    match op {
        // if operation must have a then clause
        Op::Beq | Op::Bne => {
            if let Expr::Sexp(Op::Then, _, _) = right {
                Ok(Expr::Sexp(op, Box::new(left), Box::new(right)))
            } else {
                Err(Error::from(
                    format!("if requires (then _ _) on right: {:?}", right),
                ))
            }
        }
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
        _ => Ok(Expr::Sexp(op, Box::new(left), Box::new(right))),
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

use nom::{alpha, digit};
use std::str::FromStr;
named!(
    num<u32>,
    map_res!(
        map_res!(digit, str::from_utf8),
        FromStr::from_str
    )
);

named!(
    atom<Result<Expr>>,
    ws!(do_parse!(
        val: alt_complete!(
            tag!("true")  => { |_| Ok(Prim::Bool(true)) }  |
            tag!("false") => { |_| Ok(Prim::Bool(false)) } |
            num => { |n: u32| Ok(Prim::Num(n)) } |
            alpha => { |n: &[u8]| match String::from_utf8(n.to_vec()) {
                Ok(s) => Ok(Prim::Name(s)),
                Err(e) => Err(Error::from(e)),
            } } |
            take!(1) => { |f: &[u8]| Err(Error::from(format!("unexpected token {:?}", f))) }
            ) >>
        (val.and_then(|t| Ok(Expr::Atom(t))))
    ))
);

named!(
    expr<Result<Expr>>,
    alt_complete!(sexp | atom)
);

use std::collections::HashMap;
impl Expr {
    pub fn new(src: &[u8]) -> (&[u8], Result<Self>) {
        use nom::Needed;
        match expr(src) {
            IResult::Done(rest, me) => (rest, me),
            IResult::Error(e) => (&[], Err(Error::from(e))),
            IResult::Incomplete(Needed::Unknown) => (
                &[],
                Err(Error::from(String::from("need more src"))),
            ),
            IResult::Incomplete(Needed::Size(s)) => (
                &[],
                Err(
                    Error::from(format!("need {} more bytes", s)),
                ),
            ),
        }
    }

    fn check_type_with_scope(&self, scope: &mut HashMap<Type, Type>) -> Result<Type> {
        match self {
            &Expr::Atom(ref t) => {
                match t {
                    &Prim::Bool(_) => Ok(Type::Bool),
                    &Prim::Name(ref name) => {
                        if let Some(bind) = scope.get(&Type::Var(name.clone())) {
                            Ok(bind.clone())
                        } else {
                            Ok(Type::Name(name.clone()))
                        }
                    }
                    &Prim::Num(_) => Ok(Type::Num),
                    &Prim::None => Ok(Type::None),
                }
            }
            &Expr::Sexp(ref o, ref left, ref right) => {
                let left_type: Type = left.check_type_with_scope(scope)?;
                let right_type: Type = right.check_type_with_scope(scope)?;

                match o {
                    &Op::Add | &Op::Div | &Op::Mul | &Op::Sub => {
                        if let (&Type::Num, &Type::Num) = (&left_type, &right_type) {
                            Ok(Type::Num)
                        } else {
                            Err(Error::from(
                                format!("expected (num, num) got {:?}", (left_type, right_type)),
                            ))
                        }
                    }
                    &Op::Beq | &Op::Bne => {
                        if let &Type::Bool = &left_type {
                            Ok(right_type)
                        } else {
                            Err(Error::from(
                                format!("expected (bool, _) got {:?}", (left_type, right_type)),
                            ))
                        }
                    }
                    &Op::Bind => {
                        match &left_type {
                            &Type::Name(ref n) => {
                                scope.insert(Type::Var(n.clone()), right_type);
                                Ok(Type::Var(n.clone()))
                            }
                            &Type::Var(ref v) => Err(
                                Error::from(format!("already bound: {:?}", v)),
                            ),
                            _ => {
                                Err(Error::from(
                                    format!("expected (name, _) got {:?}", (left_type.clone(), right_type)),
                                ))
                            }

                        }
                    }
                    &Op::Equiv | &Op::Gt | &Op::Lt => {
                        match (&left_type, &right_type) {
                            (&Type::Num, &Type::Num) |
                            (&Type::Bool, &Type::Bool) => Ok(Type::Bool),
                            _ => Err(Error::from(
                                format!("expected (num, num) got {:?}", (left_type, right_type)),
                            )),
                        }
                    }
                    &Op::Let => {
                        if let &Type::Var(_) = &left_type {
                            Ok(right_type)
                        } else {
                            Err(Error::from(
                                format!("expected (Var, _) got {:?}", (left_type, right_type)),
                            ))
                        }
                    }
                    &Op::Then => {
                        if left_type == right_type {
                            Ok(left_type)
                        } else {
                            Err(Error::from(
                                format!("expected (<T>, <T>) got {:?}", (left_type, right_type)),
                            ))
                        }
                    }
                }
            }
        }
    }

    // recursively check type of Expr.
    pub fn check_type(&self) -> Result<Type> {
        self.check_type_with_scope(&mut HashMap::new())
    }
}

#[cfg(test)]
mod tests;
