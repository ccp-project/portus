use std::collections::HashMap;
use super::{Error, Result};
use super::ast::{Expr, Op, Prim};
use super::prog::Prog;

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
pub struct Event {
    pub flag_idx: u32,
    pub num_flag_instrs: u32,
    pub body_idx: u32,
    pub num_body_instrs: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instr {
    pub res: Reg,
    pub op: Op,
    pub left: Reg,
    pub right: Reg,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bin {
    pub events: Vec<Event>,
    pub instrs: Vec<Instr>,
}

impl Bin {
    /// Take a `Prog`, which is a `Vec<portus::lang::prog::Event>`, and turn it into
    /// a `Bin`, which is a `Vec<portus::lang::datapath::Event>` and a `Vec<Instr>`.
    pub fn compile_prog(p: &Prog, mut scope: &mut Scope) -> Result<Self> {
        let def_instrs = scope.clone().into_iter().collect::<Vec<Instr>>();
        let mut curr_idx = def_instrs.len() as u32;

        // this is ugly
        // there might be some way to do this without all the intermediate `.collect()`
        // to turn Vec<Result<_>> into Result<Vec<_>>.
        let ls: Result<Vec<(Event, Vec<Instr>)>> = p.0
            .iter()
            .map(|ev| {
                scope.clear_tmps();
                let flag_instrs = compile_expr(&ev.flag, &mut scope).and_then(|t| {
                    let (mut instrs, res) = t;
                    // assign the flag value to the EventFlag reg.
                    let flag_reg = scope.get("eventFlag").unwrap();
                    match res {
                        Reg::Tmp(_, Type::Bool(_)) => {
                            if let Some(last) = instrs.last_mut() {
                                (*last).res = flag_reg.clone();
                            } else {
                                return Err(Error(String::from("Empty instruction list")));
                            }
                                
                            Ok(instrs)
                        }
                        Reg::ImmBool(_) => {
                            instrs.push(
                                Instr{
                                    res: flag_reg.clone(),
                                    op: Op::Bind,
                                    left: flag_reg.clone(),
                                    right: res,
                                }
                            );

                            Ok(instrs)
                        }
                        Reg::Perm(_, _) => unreachable!(),
                        x => {
                            Err(Error::from(format!("Flag expression must result in bool: {:?}", x)))
                        }
                    }
                })?;
                let num_flag_instrs = flag_instrs.len() as u32;

                let body_instrs_nested: Result<Vec<Vec<Instr>>> = ev.body.iter().map(|expr| {
                    scope.clear_tmps();
                    compile_expr(expr, &mut scope).map(|t| t.0) // Result<Vec<Instr>>
                }).collect(); // do this intermediate collect to go from Vec<Result<Vec<Instr>>> -> Result<Vec<Vec<Instr>>>

                // flatten the Vec<Vec<Instr>>
                let body_instrs: Vec<Instr> = body_instrs_nested?
                    .into_iter()
                    .flat_map(|x| x.into_iter())
                    .collect();

                let new_event = Event{
                    flag_idx: curr_idx,
                    num_flag_instrs,
                    body_idx: curr_idx + num_flag_instrs,
                    num_body_instrs: body_instrs.len() as u32,
                };

                curr_idx += new_event.num_flag_instrs + new_event.num_body_instrs;
                Ok((
                    new_event,
                    flag_instrs.into_iter().chain(body_instrs).collect()
                ))
            }).collect();

        let (evs, instrs): (Vec<_>, Vec<_>) = ls?.into_iter().unzip();
        Ok(Bin{
            events: evs,
            instrs: def_instrs.into_iter().chain(
                instrs.into_iter().flat_map(|x| x.into_iter())
            ).collect(),
        })
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
                        left,
                        right,
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
                        left,
                        right,
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
                                right,
                            });

                            Ok((instrs, left))
                        }
                        _ => Err(Error::from(format!(
                            "expected mutable register in bind, found {:?}",
                            left
                        ))),
                    }
                }
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
                        left,
                        right,
                    });

                    Ok((instrs, Reg::None))
                }
                Op::Reset => {
                    let res = scope.new_tmp(Type::Bool(None));
                    instrs.push(Instr {
                        res: res.clone(),
                        op: *o,
                        left: Reg::ImmBool(false),
                        right: Reg::ImmBool(false),
                    });

                    Ok((instrs, res))
                }
                Op::Def => unreachable!(),
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub(crate) named: HashMap<String, Reg>,
    pub(crate) num_perm: u8,
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
            "Ack.bytes_misordered"    =>  Type::Num(None),
            "Ack.ecn_bytes"           =>  Type::Num(None),
            "Ack.ecn_packets"         =>  Type::Num(None),
            "Ack.lost_pkts_sample"    =>  Type::Num(None),
            "Ack.now"                 =>  Type::Num(None),
            "Ack.packets_acked"       =>  Type::Num(None),
            "Ack.packets_misordered"  =>  Type::Num(None),
            "Flow.bytes_in_flight"    =>  Type::Num(None),
            "Flow.bytes_pending"      =>  Type::Num(None),
            "Flow.packets_in_flight"  =>  Type::Num(None),
            "Flow.rate_incoming"      =>  Type::Num(None),
            "Flow.rate_outgoing"      =>  Type::Num(None),
            "Flow.rtt_sample_us"      =>  Type::Num(None),
            "Flow.was_timeout"        =>  Type::Bool(None)
        );

        // implicit return registers (can bind to these without Def)

        // If shouldReport is true after fold function runs:
        // - immediately send the measurement to CCP, bypassing send pattern
        // - reset it to false
        // By default, Cwnd is set to the value that the send pattern sets,
        // like a constant.
        // However, if a fold function writes to Cwnd, the
        // congestion window is updated, just as if a send pattern had changed it.
        sc.num_perm = expand_reg!(
            sc; Perm;
            "eventFlag"     =>  Type::Bool(None),
            "shouldReport"  =>  Type::Bool(None),
            "Ns"            =>  Type::Num(None),
            "Cwnd"          =>  Type::Num(None),
            "Rate"          =>  Type::Num(None)
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

    // if the Type was initially None, update it now that we know what it is.
    fn update_type(&mut self, name: &str, t: &Type) -> Result<Reg> {
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
        self.instrs.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use lang::ast::Op;
    use lang::prog::Prog;
    use super::{Bin, Event, Instr, Reg, Type};
    #[test]
    fn primitives() {
        let foo = b"
        (def (foo 0))
        (when true
            (bind Report.foo 4)
        )";
        
        let (_, sc) = Prog::new_with_scope(foo).unwrap();

        // check that the registers are where they're supposed to be so we can just get from scope after this test
        // primitive
        assert_eq!(sc.get("Ack.bytes_acked"       ).unwrap().clone(), Reg::Const(0, Type::Num(None)));
        assert_eq!(sc.get("Ack.bytes_misordered"  ).unwrap().clone(), Reg::Const(1, Type::Num(None)));
        assert_eq!(sc.get("Ack.ecn_bytes"         ).unwrap().clone(), Reg::Const(2, Type::Num(None)));
        assert_eq!(sc.get("Ack.ecn_packets"       ).unwrap().clone(), Reg::Const(3, Type::Num(None)));
        assert_eq!(sc.get("Ack.lost_pkts_sample"  ).unwrap().clone(), Reg::Const(4, Type::Num(None)));
        assert_eq!(sc.get("Ack.now"               ).unwrap().clone(), Reg::Const(5, Type::Num(None)));
        assert_eq!(sc.get("Ack.packets_acked"     ).unwrap().clone(), Reg::Const(6, Type::Num(None)));
        assert_eq!(sc.get("Ack.packets_misordered").unwrap().clone(), Reg::Const(7, Type::Num(None)));
        assert_eq!(sc.get("Flow.bytes_in_flight"  ).unwrap().clone(), Reg::Const(8, Type::Num(None)));
        assert_eq!(sc.get("Flow.bytes_pending"    ).unwrap().clone(), Reg::Const(9, Type::Num(None)));
        assert_eq!(sc.get("Flow.packets_in_flight").unwrap().clone(), Reg::Const(10, Type::Num(None)));
        assert_eq!(sc.get("Flow.rate_incoming"    ).unwrap().clone(), Reg::Const(11, Type::Num(None)));
        assert_eq!(sc.get("Flow.rate_outgoing"    ).unwrap().clone(), Reg::Const(12, Type::Num(None)));
        assert_eq!(sc.get("Flow.rtt_sample_us"    ).unwrap().clone(), Reg::Const(13, Type::Num(None)));
        assert_eq!(sc.get("Flow.was_timeout"      ).unwrap().clone(), Reg::Const(14, Type::Bool(None)));

        // implicit
        assert_eq!(sc.get("eventFlag"   ).unwrap().clone(), Reg::Perm(0, Type::Bool(None)));
        assert_eq!(sc.get("shouldReport").unwrap().clone(), Reg::Perm(1, Type::Bool(None)));
        assert_eq!(sc.get("Ns"          ).unwrap().clone(), Reg::Perm(2, Type::Num(None)));
        assert_eq!(sc.get("Cwnd"        ).unwrap().clone(), Reg::Perm(3, Type::Num(None)));
        assert_eq!(sc.get("Rate"        ).unwrap().clone(), Reg::Perm(4, Type::Num(None)));

        // state
        assert_eq!(sc.get("Report.foo").unwrap().clone(), Reg::Perm(5, Type::Num(Some(0))));
    }

    #[test]
    fn reg() { 
        let foo = b"
        (def (foo 0))
        (when true
            (bind Report.foo 4)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        
        assert_eq!(
            b,
            Bin{
                events: vec![Event{
                    flag_idx: 1,
                    num_flag_instrs: 1,
                    body_idx: 2,
                    num_body_instrs: 1,
                }],
                instrs: vec![
                    Instr {
                        res: sc.get("Report.foo").unwrap().clone(),
                        op: Op::Def,
                        left: sc.get("Report.foo").unwrap().clone(),
                        right: Reg::ImmNum(0),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: sc.get("Report.foo").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("Report.foo").unwrap().clone(),
                        right: Reg::ImmNum(4),
                    },
                ]
            }
        );
    }

    #[test]
    fn underscored_var() {
        let foo = b"
        (def (foo 0))
        (when true
            (bind Report.foo 4)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        
        // check for underscored state variable
        let foo2 = b"
        (def (foo_bar 0))
        (when true
            (bind Report.foo_bar 4)
        )
        ";
        
        let (p2, mut sc2) = Prog::new_with_scope(foo2).unwrap();
        let b2 = Bin::compile_prog(&p2, &mut sc2).unwrap();
        assert_eq!(b2, b);
    }

    #[test]
    fn optional_prefix() {
        let foo = b"
        (def (foo 0))
        (when true
            (bind Report.foo 4)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        
        // check for underscored state variable
        let foo2 = b"
        (def (Report.foo 0))
        (when true
            (bind Report.foo 4)
        )
        ";
        
        let (p2, mut sc2) = Prog::new_with_scope(foo2).unwrap();
        let b2 = Bin::compile_prog(&p2, &mut sc2).unwrap();
        assert_eq!(b2, b);
    }

    #[test]
    fn ewma() {
        let foo = b"
        (def (foo 0))
        (when true
            (bind Report.foo (ewma 2 Flow.rate_outgoing))
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();

        let foo_reg = sc.get("Report.foo").unwrap().clone();

        assert_eq!(
            b,
            Bin{
                events: vec![Event{
                    flag_idx: 1,
                    num_flag_instrs: 1,
                    body_idx: 2,
                    num_body_instrs: 1,
                }],
                instrs: vec![
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Def,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(0),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Ewma,
                        left: Reg::ImmNum(2),
                        right: sc.get("Flow.rate_outgoing").unwrap().clone(),
                    },
                ]
            }
        );
    }

    #[test]
    fn infinity_if() {
        let foo = b"
        (def (foo +infinity))
        (when true
            (bind Report.foo (if (< Flow.rtt_sample_us Report.foo) Flow.rtt_sample_us))
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let foo_reg = sc.get("Report.foo").unwrap().clone();

        assert_eq!(
            b,
            Bin{
                events: vec![Event{
                    flag_idx: 1,
                    num_flag_instrs: 1,
                    body_idx: 2,
                    num_body_instrs: 2,
                }],
                instrs: vec![
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Def,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(u64::max_value()),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: Reg::Tmp(0, Type::Bool(None)),
                        op: Op::Lt,
                        left: sc.get("Flow.rtt_sample_us").unwrap().clone(),
                        right: foo_reg.clone(),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::If,
                        left: Reg::Tmp(0, Type::Bool(None)),
                        right: sc.get("Flow.rtt_sample_us").unwrap().clone(),
                    },
                ]
            }
        );
    }

    #[test]
    fn intermediate() {
        let foo = b"
        (def (foo 0))
        (when true
            (bind bar 3)
            (bind Report.foo (+ 2 bar))
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let foo_reg = sc.get("Report.foo").unwrap().clone();

        assert_eq!(
            b,
            Bin{
                events: vec![Event{
                    flag_idx: 1,
                    num_flag_instrs: 1,
                    body_idx: 2,
                    num_body_instrs: 3,
                }],
                instrs: vec![
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Def,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(0),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: sc.get("bar").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("bar").unwrap().clone(),
                        right: Reg::ImmNum(3),
                    },
                    Instr {
                        res: Reg::Tmp(0, Type::Num(None)),
                        op: Op::Add,
                        left: Reg::ImmNum(2),
                        right: sc.get("bar").unwrap().clone(),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::Tmp(0, Type::Num(None)),
                    },
                ]
            }
        );
    }

    #[test]
    fn prog_reset_tmps() {
        let foo = b"
        (def (foo 0))
        (when (> (+ 1 2) 3)
            (bind Report.foo (+ (+ 1 2) 3))
            (bind Report.foo (+ (+ 4 5) 6))
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let foo_reg = sc.get("Report.foo").unwrap().clone();

        assert_eq!(
            b,
            Bin{
                events: vec![Event{
                    flag_idx: 1,
                    num_flag_instrs: 2,
                    body_idx: 3,
                    num_body_instrs: 6,
                }],
                instrs: vec![
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Def,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(0),
                    },
                    Instr {
                        res: Reg::Tmp(0, Type::Num(None)),
                        op: Op::Add,
                        left: Reg::ImmNum(1),
                        right: Reg::ImmNum(2),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Gt,
                        left: Reg::Tmp(0, Type::Num(None)),
                        right: Reg::ImmNum(3),
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
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
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
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::Tmp(1, Type::Num(None)),
                    },
                ]
            }
        );
    }
    
    #[test]
    fn multiple_events() {
        // check that the registers are where they're supposed to be so we can just get from scope after this test
        let foo = b"
        (def (foo 0))
        (when true
            (bind Report.foo 4)
        )
        (when (> 2 3)
            (bind Report.foo 5)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let foo_reg = sc.get("Report.foo").unwrap().clone();
        
        assert_eq!(
            b,
            Bin{
                events: vec![
                    Event{
                        flag_idx: 1,
                        num_flag_instrs: 1,
                        body_idx: 2,
                        num_body_instrs: 1,
                    },
                    Event{
                        flag_idx: 3,
                        num_flag_instrs: 1,
                        body_idx: 4,
                        num_body_instrs: 1,
                    },
                ],
                instrs: vec![
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Def,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(0),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(4),
                    },
                    Instr {
                        res: sc.get("eventFlag").unwrap().clone(),
                        op: Op::Gt,
                        left: Reg::ImmNum(2),
                        right: Reg::ImmNum(3),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(5),
                    },
                ]
            }
        );
    }
}
