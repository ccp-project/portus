use std::collections::HashMap;
use super::{Error, Result};
use super::ast::{Expr, Op, Prim, Prog};

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
    match *e {
        Expr::Atom(ref t) => {
            match *t {
                Prim::Bool(t) => Ok(Type::Bool(Some(t))),
                Prim::Name(ref name) => Ok(Type::Name(name.clone())),
                Prim::Num(n) => Ok(Type::Num(Some(n))),
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
        match *self {
            Reg::ImmNum(n) => Ok(Type::Num(Some(n))),
            Reg::ImmBool(b) => Ok(Type::Bool(Some(b))),
            Reg::Const(_, ref t) | Reg::Tmp(_, ref t) | Reg::Perm(_, ref t) => Ok(t.clone()),
            Reg::None => Ok(Type::None),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instr {
    pub res: Reg,
    pub op: Op,
    pub left: Reg,
    pub right: Reg,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bin(pub Vec<Instr>);

impl Bin {
    /// Given a Prog, call compile_expr() on each Expr, then append the resulting Vec<Instrs>
    pub fn compile_prog(p: &Prog, mut scope: &mut Scope) -> Result<Self> {
        // TODO once compile_expr returns iterator, can chain without
        // intermediate collect()
        // this is ugly
        let i = scope.clone().into_iter().collect::<Vec<Instr>>();
        p.0
            .iter()
            .map(|e| {
                scope.clear_tmps();
                compile_expr(e, &mut scope).map(|t| match t {
                    (instrs, _) => instrs,
                })
            })
            .fold(
                // TODO once compile_expr returns iterator, can just flatMap
                Ok(i),
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
            .map(Bin)
    }
}

// TODO make iterative instead of recursive, and return impl Iterator<Instr>
/// Given a single Expr, return
/// a Vec<Instr> that evaluates that Expr
/// a Reg in which the result is stored
///
/// Performs a recursive depth-first search of the Expr tree.
/// The left argument is evaluated first.
fn compile_expr(e: &Expr, mut scope: &mut Scope) -> Result<(Vec<Instr>, Reg)> {
    match *e {
        Expr::Atom(ref t) => {
            match *t {
                Prim::Bool(b) => Ok((vec![], Reg::ImmBool(b))),
                Prim::Name(ref name) => {
                    if scope.has(name) {
                        let reg = scope.get(name).unwrap();
                        Ok((vec![], reg.clone()))
                    } else {
                        Ok((
                            vec![],
                            scope.new_perm(name.clone(), Type::Name(name.clone())),
                        ))
                    }
                }
                Prim::Num(n) => Ok((vec![], Reg::ImmNum(n as u64))),
            }
        }
        Expr::Sexp(ref o, box ref left_expr, box ref right_expr) => {
            let (mut instrs, mut left) = compile_expr(left_expr, &mut scope)?;
            let (mut right_instrs, right) = compile_expr(right_expr, &mut scope)?;
            instrs.append(&mut right_instrs);
            match *o {
                Op::Add | Op::Div | Op::Max | Op::MaxWrap | Op::Min | Op::Mul | Op::Sub => {
                    // left and right should have type num
                    match left.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("{:?} expected Num, got {:?}", o, x))),
                    }
                    match right.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => {
                            return Err(Error::from(
                                format!("{:?} expected Num, got {:?}: {:?}", o, x, scope),
                            ))
                        }
                    }

                    let res = scope.new_tmp(Type::Num(None));
                    instrs.push(Instr {
                        res: res.clone(),
                        op: *o,
                        left: left,
                        right: right,
                    });

                    Ok((instrs, res))
                }
                Op::Equiv | Op::Gt | Op::Lt => {
                    // left and right should have type num
                    match left.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("{:?} expected Num, got {:?}", o, x))),
                    }
                    match right.get_type() {
                        Ok(Type::Num(_)) => (),
                        x => return Err(Error::from(format!("{:?} expected Num, got {:?}", o, x))),
                    }

                    let res = scope.new_tmp(Type::Bool(None));
                    instrs.push(Instr {
                        res: res.clone(),
                        op: *o,
                        left: left,
                        right: right,
                    });

                    Ok((instrs, res))
                }
                Op::Bind => {
                    // (bind a b) assign variable a to value b

                    // if type(left) is None, give it type of right
                    if let Ok(Type::Name(s)) = left.get_type() {
                        let right_type = right.get_type().unwrap();
                        left = scope.update_type(&s, &right_type)?;
                    }

                    // left must be a mutable register
                    // and if right is a Reg::None, we have to replace it
                    match (&left, &right) {
                        (&Reg::Perm(_, _), &Reg::None) => {
                            let last_instr = instrs.last_mut().map(|last| {
                                // Double-check that the instruction being replaced
                                // actually is a Reg::None before we go replace it
                                assert_eq!(last.res, Reg::None);
                                last.res = left.clone();
                                Some(())
                            });

                            if last_instr.is_some() {
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
                                op: *o,
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
                Op::Let => Ok((instrs, right)),
                Op::Ewma | Op::If | Op::NotIf => {
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
                        op: *o,
                        left: left,
                        right: right,
                    });

                    Ok((instrs, Reg::None))
                }
                Op::Def => unreachable!(),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    named: HashMap<String, Reg>,
    num_perm: u8,
    tmp: Vec<Reg>,
}

macro_rules! add_reg {
    ($scope:ident, $name:expr, $rtyp:ident, $idx:expr, $typ:expr) => ({
        $scope.named.insert(
            String::from($name),
            Reg::$rtyp($idx, $typ),
        );
    })
}

macro_rules! expand_reg {
    (
        $scope:ident; 
        $reg:ident; 
        $count:expr; 
        $headname:expr => $headtype:expr
    ) => (
        {add_reg!($scope, $headname, $reg, $count, $headtype); $count + 1}
    );
    (
        $scope:ident; 
        $reg:ident; 
        $count:expr; 
        $headname:expr => $headtype:expr, 
        $( $restname:expr => $resttype:expr ),*
    ) => ({
        add_reg!($scope, $headname, $reg, $count, $headtype);
        expand_reg!($scope; $reg; $count+1; $( $restname => $resttype ),* )
    });
    (
        $scope:ident; 
        $reg:ident; 
        $headname:expr => $headtype:expr, 
        $( $restname:expr => $resttype:expr ),*
    ) => ({
        add_reg!($scope, $headname, $reg, 0, $headtype);
        expand_reg!($scope; $reg; 1; $( $restname => $resttype ),* )
    });
}

impl Scope {
    /// Define variables always accessible in the datapath,
    /// in the context of the most recent packet.
    /// All datapaths shall recognize these Names.
    pub(crate) fn new() -> Self {
        let mut sc = Scope {
            named: HashMap::new(),
            num_perm: 0,
            tmp: vec![],
        };

        // available measurement primitives (alphabetical order)
        expand_reg!(
            sc; Const;
            "Ack.bytes_acked"         =>  Type::Num(None),
            "Ack.packets_acked"       =>  Type::Num(None),
            "Ack.bytes_misordered"    =>  Type::Num(None),
            "Ack.packets_misordered"  =>  Type::Num(None),
            "Ack.ecn_bytes"           =>  Type::Num(None),
            "Ack.ecn_packets"         =>  Type::Num(None),
            "Ack.lost_pkts_sample"    =>  Type::Num(None),
            "Flow.was_timeout"        =>  Type::Num(None),
            "Flow.rtt_sample_us"      =>  Type::Num(None),
            "Flow.rate_outgoing"      =>  Type::Num(None),
            "Flow.rate_incoming"      =>  Type::Num(None),
            "Flow.bytes_in_flight"    =>  Type::Num(None),
            "Flow.packets_in_flight"  =>  Type::Num(None),
            "Flow.snd_cwnd"           =>  Type::Num(None),
            "Ack.now"                 =>  Type::Num(None),
            "Flow.bytes_pending"      =>  Type::Num(None)
        );

        // implicit return registers (can bind to these without Def)

        // If isUrgent is true after fold function runs:
        // - immediately send the measurement to CCP, bypassing send pattern
        // - reset it to false
        // By default, Cwnd is set to the value that the send pattern sets,
        // like a constant.
        // However, if a fold function writes to Cwnd, the
        // congestion window is updated, just as if a send pattern had changed it.
        sc.num_perm = expand_reg!(
            sc; Perm;
            "isUrgent"  =>  Type::Bool(None),
            "Cwnd"      =>  Type::Num(None)
        );
        
        sc
    }

    pub fn has(&self, name: &str) -> bool {
        self.named.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&Reg> {
        self.named.get(name)
    }

    pub(crate) fn new_tmp(&mut self, t: Type) -> Reg {
        let id = self.tmp.len() as u8;
        let r = Reg::Tmp(id, t);
        self.tmp.push(r);
        self.tmp[id as usize].clone()
    }

    pub(crate) fn new_perm(&mut self, name: String, t: Type) -> Reg {
        let id = self.num_perm;
        self.num_perm += 1;
        let r = Reg::Perm(id, t);
        self.named.insert(name, r.clone());
        r
    }

    /// if the Type was initially None, update it now that we know what it is.
    pub(crate) fn update_type(&mut self, name: &str, t: &Type) -> Result<Reg> {
        self.named
            .get_mut(name)
            .ok_or_else(|| Error::from(format!("Unknown {:?}", name)))
            .and_then(|old_reg| match *old_reg {
                Reg::Perm(idx, Type::Name(_)) => {
                    *old_reg = Reg::Perm(idx, t.clone());
                    Ok(old_reg.clone())
                }
                _ => Err(Error::from(format!(
                    "update_type: only Perm(_, Type::None) allowed: {:?}",
                    old_reg
                ))),
            })

    }

    pub(crate) fn clear_tmps(&mut self) {
        self.tmp.clear()
    }
}

pub struct ScopeDefInstrIter {
    v: ::std::collections::hash_map::IntoIter<String, Reg>,
}

impl ScopeDefInstrIter {
    fn new(it: ::std::collections::hash_map::IntoIter<String, Reg>) -> Self {
        ScopeDefInstrIter { v: it }
    }
}

impl Iterator for ScopeDefInstrIter {
    type Item = Instr;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (name, reg) = self.v.next()?;
            if !name.as_str().starts_with("Report.") {
                continue;
            }

            match reg {
                Reg::Perm(_, Type::Num(Some(n))) => {
                    return Some(Instr {
                        res: reg.clone(),
                        op: Op::Def,
                        left: reg.clone(),
                        right: Reg::ImmNum(n),
                    })
                }
                Reg::Perm(_, Type::Bool(Some(b))) => {
                    return Some(Instr {
                        res: reg.clone(),
                        op: Op::Def,
                        left: reg.clone(),
                        right: Reg::ImmBool(b),
                    })
                }
                Reg::Perm(_, _) => continue, // implicit bool register
                _ => unreachable!(),
            }
        }
    }
}

impl IntoIterator for Scope {
    type Item = Instr;
    type IntoIter = ScopeDefInstrIter;

    fn into_iter(self) -> ScopeDefInstrIter {
        ScopeDefInstrIter::new(self.named.into_iter())
    }
}

impl IntoIterator for Bin {
    type Item = Instr;
    type IntoIter = ::std::vec::IntoIter<Instr>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use lang::ast::{Op, Prog};
    use super::{Bin, Instr, Type, Reg};
    #[test]
    fn prog() {
        let foo = b"
        (def (foo 0))
        (bind Report.foo 4)
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(4),
                },
            ])
        );
    }

    #[test]
    fn def() {
        let foo = b"
        (def (foo 0))
        (bind Report.foo (+ 2 3))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(2),
                    right: Reg::ImmNum(3),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::Tmp(0, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn ewma() {
        let foo = b"
        (def (foo 0))
        (bind Report.foo (ewma 2 Flow.rate_outgoing))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Ewma,
                    left: Reg::ImmNum(2),
                    right: Reg::Const(9, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn infinity_if() {
        let foo = b"
        (def (foo +infinity))
        (bind Report.foo (if (< Flow.rtt_sample_us Report.foo) Flow.rtt_sample_us))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(u64::max_value()))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(u64::max_value()))),
                    right: Reg::ImmNum(u64::max_value()),
                },
                Instr {
                    res: Reg::Tmp(0, Type::Bool(None)),
                    op: Op::Lt,
                    left: Reg::Const(8, Type::Num(None)),
                    right: Reg::Perm(2, Type::Num(Some(u64::max_value()))),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(u64::max_value()))),
                    op: Op::If,
                    left: Reg::Tmp(0, Type::Bool(None)),
                    right: Reg::Const(8, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn intermediate() {
        let foo = b"
        (def (foo 0))
        (bind bar 3)
        (bind Report.foo (+ 2 bar))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Perm(3, Type::Num(Some(3))),
                    op: Op::Bind,
                    left: Reg::Perm(3, Type::Num(Some(3))),
                    right: Reg::ImmNum(3),
                },
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(2),
                    right: Reg::Perm(3, Type::Num(Some(3))),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::Tmp(0, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn prog_reset_tmps() {
        let foo = b"
        (def (foo 0))
        (bind Report.foo (+ (+ 1 2) 3))
        (bind Report.foo (+ (+ 4 5) 6))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
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
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
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
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::Tmp(1, Type::Num(None)),
                },
            ])
        );
    }

    #[test]
    fn underscored_state_variable() {
        let foo = b"
            (def (foo_bar 0))
			(bind Report.foo_bar (+ 1 2))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(1),
                    right: Reg::ImmNum(2),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::Tmp(0, Type::Num(None)),
                },
            ])
        );
    }
    
	#[test]
    fn optional_flow_prefix() {
        let foo = b"
            (def (Report.foo_bar 0))
			(bind Report.foo_bar (+ 1 2))
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        assert_eq!(
            b,
            Bin(vec![
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Def,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Tmp(0, Type::Num(None)),
                    op: Op::Add,
                    left: Reg::ImmNum(1),
                    right: Reg::ImmNum(2),
                },
                Instr {
                    res: Reg::Perm(2, Type::Num(Some(0))),
                    op: Op::Bind,
                    left: Reg::Perm(2, Type::Num(Some(0))),
                    right: Reg::Tmp(0, Type::Num(None)),
                },
            ])
        );
    }
}
