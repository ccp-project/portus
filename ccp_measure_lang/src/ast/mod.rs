use std;
use nom::IResult;
use super::{Error, Result};

#[derive(Clone)]
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
    Tup(Box<Type>, Box<Type>),
}

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq)]
pub enum Op {
    Add, // (add a b) return a+b
    Beq, // (if a (tup b c)) if a is true, return b, else c
    Bind, // (bind a b) assign variable a to value b
    Bne, // (if a (tup b c)) if a is false, return b, else c
    Div, // (div a b) return a/b (integer division)
    Equiv, // (eq a b) return a == b
    Ewma, // (ewma a (tup b c)) return b * (a/10) + c * (1-(a/10))
    Gt, // (> a b) return a > b
    Let, // (let (bind a b) c) with variable a bound to b, evaluate c
    Lt, // (< a b) return a < b
    Max, // (max a b) return max(a,b)
    Min, // (min a b) return min(a,b)
    Mul, // (mul a b) return a * b
    Sub, // (sub a b) return a - b
    Tup, // (tup a b) create tuple (a,b)
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
        alt!(tag!("+") | tag!("add"))   => { |_| Ok(Op::Add) }   |
        tag!("if")                      => { |_| Ok(Op::Beq) }   | 
        alt!(tag!(":=") | tag!("bind")) => { |_| Ok(Op::Bind) }  | 
        tag!("!if")                     => { |_| Ok(Op::Bne) }   | 
        alt!(tag!("/") | tag!("div"))   => { |_| Ok(Op::Div) }   | 
        alt!(tag!("==") | tag!("eq"))   => { |_| Ok(Op::Equiv) } | 
        tag!("ewma")                    => { |_| Ok(Op::Ewma) }  | 
        alt!(tag!(">") | tag!("gt"))    => { |_| Ok(Op::Gt) }    | 
        tag!("let")                     => { |_| Ok(Op::Let) }   | 
        alt!(tag!("<") | tag!("lt"))    => { |_| Ok(Op::Lt) }    | 
        tag!("max")                     => { |_| Ok(Op::Max) }   | 
        tag!("min")                     => { |_| Ok(Op::Min) }   | 
        alt!(tag!("*") | tag!("mul"))   => { |_| Ok(Op::Mul) }   |
        alt!(tag!("-") | tag!("sub"))   => { |_| Ok(Op::Sub) }   |
        tag!("tup")                     => { |_| Ok(Op::Tup) }   | 
        atom => { |f: Result<Expr>| Err(Error::from(format!("unexpected token {:?}", f))) }
    )
);

fn check_expr(op: Op, left: Expr, right: Expr) -> Result<Expr> {
    match op {
        // multi-argument operations
        Op::Beq | Op::Bne | Op::Ewma => {
            if let Expr::Sexp(Op::Tup, _, _) = right {
                Ok(Expr::Sexp(op, Box::new(left), Box::new(right)))
            } else {
                Err(Error::from(
                    format!("if requires (tup _ _) on right: {:?}", right),
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

use nom::digit;
use std::str::FromStr;
named!(
    num<u32>,
    map_res!(
        map_res!(digit, str::from_utf8),
        FromStr::from_str
    )
);

use nom::is_alphanumeric;
named!(
    name<&[u8]>,
    take_while1!(|u: u8| is_alphanumeric(u) || u == '.' as u8)
);

use nom::rest;
named!(
    atom<Result<Expr>>,
    ws!(do_parse!(
        val: alt_complete!(
            tag!("true")  => { |_| Ok(Prim::Bool(true)) }  |
            tag!("false") => { |_| Ok(Prim::Bool(false)) } |
            num => { |n: u32| Ok(Prim::Num(n)) } |
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

mod scope;
use self::scope::Scope;

impl Expr {
    pub fn new(src: &[u8]) -> Result<Vec<Self>> {
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

    fn check_type_with_scope(&self, scope: &mut Scope) -> Result<Type> {
        println!("exp: {:?}, scope: {:?}", self, scope);
        match self {
            &Expr::Atom(ref t) => {
                match t {
                    &Prim::Bool(_) => Ok(Type::Bool),
                    &Prim::Name(ref name) => {
                        match scope.get(&Type::Name(name.clone())) {
                            Some(bind) if bind != &Type::None => Ok(bind.clone()),
                            _ => Ok(Type::Name(name.clone())),
                        }
                    }
                    &Prim::Num(_) => Ok(Type::Num),
                    &Prim::None => Ok(Type::None),
                }
            }
            &Expr::Sexp(ref o, ref left, ref right) => {
                match o {
                    &Op::Add | &Op::Div | &Op::Max | &Op::Min | &Op::Mul | &Op::Sub => {
                        let left_type: Type = left.check_type_with_scope(scope)?;
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        if let (&Type::Num, &Type::Num) = (&left_type, &right_type) {
                            Ok(Type::Num)
                        } else {
                            Err(Error::from(
                                format!("expected (num, num) got {:?}", (left_type, right_type)),
                            ))
                        }
                    }
                    &Op::Beq | &Op::Bne => {
                        let left_type: Type = left.check_type_with_scope(scope)?;
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        match (&left_type, &right_type) {
                            (&Type::Bool, &Type::Tup(box Type::Bool, box Type::Bool)) => {
                                Ok(Type::Bool)
                            }
                            (&Type::Bool, &Type::Tup(box Type::Num, box Type::Num)) => {
                                Ok(Type::Num)
                            }
                            _ => Err(Error::from(
                                format!("expected bool, (<T>, <T>)) got {:?}", (&left_type, &right_type)),
                            )),
                        }
                    }
                    &Op::Bind => {
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        match **left {
                            Expr::Atom(Prim::Name(ref n)) => {
                                match scope.bind(Type::Name(n.clone()), right_type) {
                                    Some(ref b) if b != &Type::None => Err(Error::from(
                                        format!("already bound: {:?}", n),
                                    )),
                                    _ => Ok(Type::Name(n.clone())),
                                }
                            }
                            _ => {
                                Err(Error::from(
                                    format!("expected (name, _) got {:?}", (left, right_type)),
                                ))
                            }
                        }
                    }
                    &Op::Equiv | &Op::Gt | &Op::Lt => {
                        let left_type: Type = left.check_type_with_scope(scope)?;
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        match (&left_type, &right_type) {
                            (&Type::Num, &Type::Num) |
                            (&Type::Bool, &Type::Bool) => Ok(Type::Bool),
                            _ => Err(Error::from(
                                format!("expected (num, num) got {:?}", (left_type, right_type)),
                            )),
                        }
                    }
                    &Op::Ewma => {
                        let left_type: Type = left.check_type_with_scope(scope)?;
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        match (&left_type, &right_type) {
                            (&Type::Num, &Type::Tup(box Type::Num, box Type::Num)) => Ok(Type::Num),
                            _ => Err(Error::from(
                                format!("expected Num, (Num, Num)) got {:?}", (&left_type, &right_type)),
                            )),
                        }
                    }
                    &Op::Let => {
                        let left_type: Type = left.check_type_with_scope(scope)?;
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        if let &Type::Name(_) = &left_type {
                            Ok(right_type)
                        } else {
                            Err(Error::from(
                                format!("expected (Name, _) got {:?}", (left_type, right_type)),
                            ))
                        }
                    }
                    &Op::Tup => {
                        let left_type: Type = left.check_type_with_scope(scope)?;
                        let right_type: Type = right.check_type_with_scope(scope)?;
                        Ok(Type::Tup(Box::new(left_type), Box::new(right_type)))
                    }
                }
            }
        }
    }

    // recursively check type of Expr.
    pub fn check_type(&self) -> Result<Type> {
        self.check_type_with_scope(&mut Scope::datapath_scope())
    }
}

/// a Prog is multiple Expr in sequence.
/// Scope cascades through the Expr:
/// Expr with Type::Name will in scope for successive Expr
/// Other Expr will not be evaluated.
#[derive(Debug)]
pub struct Prog(pub Vec<Expr>);

use nom::alpha;
/// Declare a state variable and provide an initial value
/// (Foo 0) (Bar true)
named!(
    decl<(Type, Type)>,
    delimited!(
        tag!("("),
        tuple!(
            map_res!(alpha, |b| str::from_utf8(b).and_then(|i| Ok(Type::Name(String::from(i))))),
            map_res!(atom, |a: Result<Expr>| a.and_then(|i| i.check_type()))
        ),
        tag!(")")
    )
);
/// a Prog has special syntax *at the beginning* to declare the Flow state variables.
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
                        x@ Type::Num | x@ Type::Bool => (name, x),
                        _ => (name, Type::None)
                    } 
                }).collect()
            )
        ),
        tag!(")")
    ))
);

impl Prog {
    fn new_with_scope(source: &[u8]) -> Result<(Self, Scope)> {
        let mut scope = Scope::datapath_scope();
        use nom::{IResult, Needed};
        let rest = match defs(source) {
            IResult::Done(rest, flow_state) => {
                flow_state
                    .into_iter()
                    .map(|(var, typ)| match var {
                        Type::Name(v) => (Type::Name(format!("Flow.{}", v)), typ),
                        _ => unreachable!(),
                    })
                    .for_each(|(var, typ)| { scope.init(var, typ); });

                Ok(rest)
            }
            IResult::Error(e) => Err(Error::from(e)),
            IResult::Incomplete(Needed::Unknown) => Err(Error::from(String::from("need more src"))),
            IResult::Incomplete(Needed::Size(s)) => Err(
                Error::from(format!("need {} more bytes", s)),
            ),
        }?;

        let exprs = Expr::new(rest)?
            .iter()
            .filter(|&expr| match expr.check_type_with_scope(&mut scope) {
                Ok(Type::Name(_)) => true,
                a => {
                    println!("{:?}", a);
                    false
                }
            })
            .map(|e| e.clone())
            .collect();

        Ok((Prog(exprs), scope))
    }

    pub fn new(source: &[u8]) -> Result<Self> {
        Self::new_with_scope(source).map(|t| t.0)
    }
}

#[cfg(test)]
mod test;
