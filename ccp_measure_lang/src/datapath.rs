use super::{Error, Result};
use super::prog::Prog;
use super::ast::{Expr, Op, Prim};
use super::scope::Scope;

#[derive(Clone)]
#[derive(Debug)]
#[derive(PartialEq, Eq, Hash)]
pub enum Type {
    Bool(Option<bool>),
    Name(String),
    Num(Option<u64>),
    None,
}

pub(crate) fn check_atom_type(e: &Expr) -> Result<Type> {
    match e {
        &Expr::Atom(ref t) => {
            match t {
                &Prim::Bool(t) => Ok(Type::Bool(Some(t))),
                &Prim::Name(ref name) => Ok(Type::Name(name.clone())),
                &Prim::Num(n) => Ok(Type::Num(Some(n))),
                &Prim::None => Ok(Type::None),
            }
        }
        _ => Err(Error::from(format!("not an atom: {:?}", e))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Reg {
    ImmNum(u64),
    ImmBool(bool),
    Const(u8, Type),
    Tmp(u8, Type),
    Perm(u8, Type),
    None,
}

impl Reg {
    fn get_type(&self) -> Result<Type> {
        match self {
            &Reg::ImmNum(n) => Ok(Type::Num(Some(n))),
            &Reg::ImmBool(b) => Ok(Type::Bool(Some(b))),
            &Reg::Const(_, ref t) => Ok(t.clone()),
            &Reg::Tmp(_, ref t) => Ok(t.clone()),
            &Reg::Perm(_, ref t) => Ok(t.clone()),
            &Reg::None => Ok(Type::None),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Instr {
    res: Reg,
    op: Op,
    left: Reg,
    right: Reg,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Bin(pub Vec<Instr>);

impl Bin {
    /// Given a Prog, call compile_expr() on each Expr, then append the resulting Vec<Instrs>
    pub fn compile_prog(p: &Prog, mut scope: &mut Scope) -> Result<Self> {
        p.0
            .iter()
            .map(|e| {
                scope.clear_tmps();
                compile_expr(&e, &mut scope).map(|t| match t {
                    (instrs, _) => instrs,
                })
            })
            .fold(
                // TODO once compile_expr returns iterator, can just flatMap
                Ok(vec![]),
                |acc, rv| match rv { 
                    Ok(mut v) => {
                        acc.and_then(|mut x| {
                            x.append(&mut v);
                            Ok(x)
                        })
                    }
                    e => e,
                },
            )
            .map(|e| Bin(e))
    }
}

// TODO make iterative instead of recursive, and return Iter<Instr>
/// Given a single Expr, return
/// a Vec<Instr> that evaluates that Expr
/// a Reg in which the result is stored
///
/// Performs a recursive depth-first search of the Expr tree.
/// The left argument is evaluated first.
fn compile_expr(e: &Expr, mut scope: &mut Scope) -> Result<(Vec<Instr>, Reg)> {
    match e {
        &Expr::Atom(ref t) => {
            match t {
                &Prim::Bool(b) => Ok((vec![], Reg::ImmBool(b))),
                &Prim::Name(ref name) => {
                    match scope.get(&name) {
                        Some(reg) => Ok((vec![], reg.clone())),
                        _ => Err(Error::from(format!("unknown name {:?}", name))),
                    }
                }
                &Prim::Num(n) => Ok((vec![], Reg::ImmNum(n as u64))),
                &Prim::None => Err(Error::from("unexpected None")),
            }
        }
        &Expr::Sexp(ref o, box ref left_expr, box ref right_expr) => {
            let (mut instrs, left) = compile_expr(&left_expr, &mut scope)?;
            let (mut right_instrs, right) = compile_expr(&right_expr, &mut scope)?;
            instrs.append(&mut right_instrs);
            match o {
                &Op::Add | &Op::Div | &Op::Max | &Op::Min | &Op::Mul | &Op::Sub => {
                    // left and right should have type num
                    match left.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("expected Num, got {:?}", x))),
                    }
                    match right.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("expected Num, got {:?}", x))),
                    }

                    let res = scope.new_tmp(Type::Num(None));
                    instrs.push(Instr {
                        res: res.clone(),
                        op: o.clone(),
                        left: left,
                        right: right,
                    });

                    Ok((instrs, res))
                }
                &Op::Equiv | &Op::Gt | &Op::Lt => {
                    // left and right should have type num
                    match left.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("expected Num, got {:?}", x))),
                    }
                    match right.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("expected Num, got {:?}", x))),
                    }

                    let res = scope.new_tmp(Type::Bool(None));
                    instrs.push(Instr {
                        res: res.clone(),
                        op: o.clone(),
                        left: left,
                        right: right,
                    });

                    Ok((instrs, res))
                }
                &Op::Bind => {
                    // (bind a b) assign variable a to value b
                    // left must be a mutable register
                    // and if right is a Reg::None, we have to replace it
                    match (&left, &right) {
                        (&Reg::Perm(_, _), &Reg::None) => {
                            if let Some(_) = instrs.last_mut().map(|last| {
                                // Double-check that the instruction being replaced
                                // actually is a Reg::None before we go replace it
                                assert_eq!(last.res, Reg::None);
                                last.res = left.clone();
                                Some(())
                            })
                            {
                                Ok((instrs, left))
                            } else {
                                // It's impossible to have both a Reg::None to match against
                                // and also no last instruction
                                unreachable!();
                            }
                        }
                        (&Reg::Tmp(_, _), &Reg::None) => Err(Error::from(format!(
                            "cannot bind stateful instruction to Reg::Tmp: {:?}",
                            right_expr,
                        ))),
                        (&Reg::Perm(_, _), _) |
                        (&Reg::Tmp(_, _), _) => {
                            instrs.push(Instr {
                                res: left.clone(),
                                op: o.clone(),
                                left: left.clone(),
                                right: right,
                            });

                            Ok((instrs, left))
                        }
                        _ => Err(Error::from(format!(
                            "expected mutable register in bind, found {:?}",
                            left
                        ))),
                    }
                }
                &Op::Let => Ok((instrs, right)),
                &Op::Ewma | &Op::If | &Op::NotIf => {
                    // ewma: SPECIAL: reads return register
                    // (ewma a b) ret * a/10 + b * (10-a)/10.
                    // If|NotIf: SPECIAL: cannot be bound to temp register
                    // If: (if a b) if a == True, evaluate b (write return register), otherwise don't write return register
                    // NotIf: (!if a b) if a == False, evaluate b (write return register), otherwise don't write return register
                    // Use Reg::None as a placeholder, replaced by the parent Expr node.
                    // parent Expr node must be an Op::Bind;
                    // i.e., binding into a Tmp register is not allowed
                    instrs.push(Instr {
                        res: Reg::None,
                        op: o.clone(),
                        left: left,
                        right: right,
                    });

                    Ok((instrs, Reg::None))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ast::Op;
    use prog::Prog;
    use super::{Bin, Instr, Type, Reg};
    #[test]
    fn prog() {
        let foo = b"
        (def (foo 0))
        (bind Flow.foo 4)
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(0, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(0, Type::Num(Some(0))),
                    right: Reg::ImmNum(4),
                },
            ])
        );
    }

    #[test]
    fn prog1() {
        let foo = b"
        (def (foo 0))
        (bind Flow.foo (+ 2 3))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(2),
                    right: Reg::ImmNum(3),
                },
                Instr {
                    res: Reg::Perm(0, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(0, Type::Num(Some(0))),
                    right: Reg::Tmp(0, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn prog2() {
        let foo = b"
        (def (foo 0))
        (bind Flow.foo (ewma 2 SndRate))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(0, Type::Num(Some(0))),
                    op: Op::Ewma,
                    left: Reg::ImmNum(2),
                    right: Reg::Const(2, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn prog3() {
        let foo = b"
        (def (foo 100000000))
        (bind Flow.foo (if (< Rtt Flow.foo) Rtt))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Tmp(0, Type::Bool(None)),
                    op: Op::Lt,
                    left: Reg::Const(0, Type::Num(None)),
                    right: Reg::Perm(0, Type::Num(Some(100000000))),
                },
                Instr {
                    res: Reg::Perm(0, Type::Num(Some(100000000))),
                    op: Op::If,
                    left: Reg::Tmp(0, Type::Bool(None)),
                    right: Reg::Const(0, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn prog_reset_tmps() {
        let foo = b"
        (def (foo 0) (bar 0))
        (bind Flow.foo (+ (+ 1 2) 3))
        (bind Flow.bar (+ (+ 4 5) 6))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(1),
                    right: Reg::ImmNum(2),
                },
                Instr {
                    res: Reg::Tmp(1, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::Tmp(0, Type::Num(None)),
                    right: Reg::ImmNum(3),
                },
                Instr {
                    res: Reg::Perm(0, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(0, Type::Num(Some(0))),
                    right: Reg::Tmp(1, Type::Num(None)),
                },
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(4),
                    right: Reg::ImmNum(5),
                },
                Instr {
                    res: Reg::Tmp(1, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::Tmp(0, Type::Num(None)),
                    right: Reg::ImmNum(6),
                },
                Instr {
                    res: Reg::Perm(1, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(1, Type::Num(Some(0))),
                    right: Reg::Tmp(1, Type::Num(None)),
                },
            ])
        );
    }
}
