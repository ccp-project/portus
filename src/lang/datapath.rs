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
/// A datapath register. 
pub enum Reg {
    Control(u8, Type),
    ImmNum(u64),
    ImmBool(bool),
    Implicit(u8, Type),
    Local(u8, Type),
    Primitive(u8, Type),
    Report(u8, Type, bool),
    Tmp(u8, Type),
    None,
}

impl Reg {
    fn get_type(&self) -> Result<Type> {
        match *self {
            Reg::ImmNum(n)           => Ok(Type::Num(Some(n))),
            Reg::ImmBool(b)          => Ok(Type::Bool(Some(b))),
            Reg::Control(_, ref t)   |
            Reg::Implicit(_, ref t)  |
            Reg::Local(_, ref t)     |
            Reg::Primitive(_, ref t) | 
            Reg::Tmp(_, ref t)       | 
            Reg::Report(_, ref t, _)    => Ok(t.clone()),
            Reg::None                => Ok(Type::None),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A single event to handle in the datapath, if the flag instruction evaluates truthily.
pub struct Event {
    pub flag_idx: u32,
    pub num_flag_instrs: u32,
    pub body_idx: u32,
    pub num_body_instrs: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A single instruction to execute in the datapath.
pub struct Instr {
    pub res: Reg,
    pub op: Op,
    pub left: Reg,
    pub right: Reg,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Instruction-level representation of a datapath program.
pub struct Bin {
    pub events: Vec<Event>,
    pub instrs: Vec<Instr>,
}

impl IntoIterator for Bin {
    type Item = Instr;
    type IntoIter = ::std::vec::IntoIter<Instr>;

    fn into_iter(self) -> Self::IntoIter {
        self.instrs.into_iter()
    }
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
                    let flag_reg = scope.get("__eventFlag").unwrap();
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
                        Reg::Report(_, _, _) => unreachable!(),
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
                            scope.new_local(name.clone(), Type::Name(name.clone())),
                        ))
                    }
                }
                Prim::Num(n) => Ok((vec![], Reg::ImmNum(n as u64))),
            }
        }
        Expr::Cmd(_) | Expr::None => unreachable!(),
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
                Op::And | Op::Or => {
                    // left and right should have type num
                    match left.get_type() {
                        Ok(Type::Bool(_)) => (),
                        x => return Err(Error::from(format!("{:?} expected Bool, got {:?}", o, x))),
                    }
                    match right.get_type() {
                        Ok(Type::Bool(_)) => (),
                        x => return Err(Error::from(format!("{:?} expected Bool, got {:?}", o, x))),
                    }

                    let res = scope.new_tmp(Type::Bool(None));
                    instrs.push(Instr {
                        res: res.clone(),
                        op: match *o {
                            Op::And => Op::Mul,
                            Op::Or  => Op::Add,
                            _       => unreachable!(),
                        },
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
                        (&Reg::Report(_, _, _), &Reg::None) => {
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
                        (&Reg::Implicit(_, _), _)  |
                        (&Reg::Control(_, _), _)   |
                        (&Reg::Local(_, _), _)     |
                        (&Reg::Report(_, _, _), _) |
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
                Op::Def => unreachable!(),
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RegFile(pub(crate) Vec<(String, Reg)>);

impl RegFile {
    fn new() -> Self {
        RegFile(vec![])
    }

    fn insert(&mut self, name: String, r: Reg) {
        if let Some((idx, _)) = self.0.iter()
            .enumerate()
            .skip_while(|&(_, &(ref s, _))| *s < name)
            .next() {
            self.0.insert(idx, (name, r));
        } else {
            self.0.push((name, r));
        }
    }

    fn get<'a>(&'a self, name: &str) -> Option<&'a Reg> {
        self.0.iter().find(|&&(ref s, _)| s == name).map(|&(_, ref r)| r)
    }

    fn get_mut<'a>(&'a mut self, name: &str) -> Option<&'a mut Reg> {
        self.0.iter_mut().find(|&&mut (ref s, _)| s == name).map(|&mut(_, ref mut r)| r)
    }
}

#[derive(Clone, Debug)]
/// A mapping from variable names defined in the datapath program to their
/// datapath register representations.
pub struct Scope {
    pub(crate) named: RegFile,
    pub(crate) num_control: u8,
    pub(crate) num_local: u8,
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
    pub fn new() -> Self {
        let mut sc = Scope {
            named: RegFile::new(),
            num_control: 0,
            num_local: 0,
            num_perm: 0,
            tmp: vec![],
        };

        // available measurement primitives (alphabetical order)
        expand_reg!(
            sc; Primitive;
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

        // implicit return registers

        // If __shouldReport is true after fold function runs:
        // - immediately send the measurement to CCP
        // - reset it to false
        expand_reg!(
            sc; Implicit;
            "__eventFlag"      => Type::Bool(None),
            "__shouldContinue" => Type::Bool(None),
            "__shouldReport"   => Type::Bool(None),
            "Micros"           => Type::Num(None),
            "Cwnd"           => Type::Num(None),
            "Rate"           => Type::Num(None)
        );
        
        sc
    }

    pub fn has(&self, name: &str) -> bool {
        self.named.get(name).is_some()
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

    pub(crate) fn new_report(&mut self, is_volatile: bool, name: String, t: Type) -> Reg {
        let id = self.num_perm;
        self.num_perm += 1;
        let r = Reg::Report(id, t, is_volatile);
        self.named.insert(name, r.clone());
        r
    }

    pub(crate) fn new_control(&mut self, name: String, t: Type) -> Reg {
        let id = self.num_control;
        self.num_control += 1;
        let r = Reg::Control(id, t);
        self.named.insert(name, r.clone());
        r
    }
    
    pub(crate) fn new_local(&mut self, name: String, t: Type) -> Reg {
        let id = self.num_local;
        self.num_local += 1;
        let r = Reg::Local(id, t);
        self.named.insert(name, r.clone());
        r
    }

    // if the Type was initially None, update it now that we know what it is.
    fn update_type(&mut self, name: &str, t: &Type) -> Result<Reg> {
        self.named
            .get_mut(name)
            .ok_or_else(|| Error::from(format!("Unknown {:?}", name)))
            .and_then(|old_reg| match *old_reg {
                Reg::Report(idx, Type::Name(_), v) => {
                    *old_reg = Reg::Report(idx, t.clone(), v);
                    Ok(old_reg.clone())
                }
                Reg::Local(idx, Type::Name(_)) => {
                    *old_reg = Reg::Local(idx, t.clone());
                    Ok(old_reg.clone())
                }
                Reg::Control(idx, Type::Name(_)) => {
                    *old_reg = Reg::Control(idx, t.clone());
                    Ok(old_reg.clone())
                }
                _ => Err(Error::from(format!(
                    "update_type: only Report,Local,Control allowed: {:?}",
                    old_reg
                ))),
            })

    }

    pub(crate) fn clear_tmps(&mut self) {
        self.tmp.clear()
    }
}

impl Default for Scope {
    fn default() -> Self {
        Scope::new()
    }
}

pub struct ScopeDefInstrIter {
    pub v: ::std::vec::IntoIter<(String, Reg)>,
}

impl Iterator for ScopeDefInstrIter {
    type Item = Instr;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (_, reg) = self.v.next()?;
            match reg {
                Reg::Report(_, Type::Num(Some(n)), _) |
                Reg::Control(_, Type::Num(Some(n))) => {
                    return Some(Instr {
                        res: reg.clone(),
                        op: Op::Def,
                        left: reg.clone(),
                        right: Reg::ImmNum(n),
                    })
                }
                Reg::Report(_, Type::Bool(Some(b)), _) |
                Reg::Control(_, Type::Bool(Some(b))) => {
                    return Some(Instr {
                        res: reg.clone(),
                        op: Op::Def,
                        left: reg.clone(),
                        right: Reg::ImmBool(b),
                    })
                }
                _ => continue,
            }
        }
    }
}

impl IntoIterator for Scope {
    type Item = Instr;
    type IntoIter = ScopeDefInstrIter;

    fn into_iter(self) -> ScopeDefInstrIter {
        ScopeDefInstrIter{ v: self.named.0.into_iter() }
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
        (def (Report (foo 0)))
        (when true
            (bind Report.foo 4)
        )";
        
        let (_, sc) = Prog::new_with_scope(foo).unwrap();

        // check that the registers are where they're supposed to be so we can just get from scope after this test
        // primitive
        assert_eq!(sc.get("Ack.bytes_acked"       ).unwrap().clone(), Reg::Primitive(0, Type::Num(None)));
        assert_eq!(sc.get("Ack.bytes_misordered"  ).unwrap().clone(), Reg::Primitive(1, Type::Num(None)));
        assert_eq!(sc.get("Ack.ecn_bytes"         ).unwrap().clone(), Reg::Primitive(2, Type::Num(None)));
        assert_eq!(sc.get("Ack.ecn_packets"       ).unwrap().clone(), Reg::Primitive(3, Type::Num(None)));
        assert_eq!(sc.get("Ack.lost_pkts_sample"  ).unwrap().clone(), Reg::Primitive(4, Type::Num(None)));
        assert_eq!(sc.get("Ack.now"               ).unwrap().clone(), Reg::Primitive(5, Type::Num(None)));
        assert_eq!(sc.get("Ack.packets_acked"     ).unwrap().clone(), Reg::Primitive(6, Type::Num(None)));
        assert_eq!(sc.get("Ack.packets_misordered").unwrap().clone(), Reg::Primitive(7, Type::Num(None)));
        assert_eq!(sc.get("Flow.bytes_in_flight"  ).unwrap().clone(), Reg::Primitive(8, Type::Num(None)));
        assert_eq!(sc.get("Flow.bytes_pending"    ).unwrap().clone(), Reg::Primitive(9, Type::Num(None)));
        assert_eq!(sc.get("Flow.packets_in_flight").unwrap().clone(), Reg::Primitive(10, Type::Num(None)));
        assert_eq!(sc.get("Flow.rate_incoming"    ).unwrap().clone(), Reg::Primitive(11, Type::Num(None)));
        assert_eq!(sc.get("Flow.rate_outgoing"    ).unwrap().clone(), Reg::Primitive(12, Type::Num(None)));
        assert_eq!(sc.get("Flow.rtt_sample_us"    ).unwrap().clone(), Reg::Primitive(13, Type::Num(None)));
        assert_eq!(sc.get("Flow.was_timeout"      ).unwrap().clone(), Reg::Primitive(14, Type::Bool(None)));

        assert_eq!(sc.get("__eventFlag"     ).unwrap().clone(), Reg::Implicit(0, Type::Bool(None)));
        assert_eq!(sc.get("__shouldContinue").unwrap().clone(), Reg::Implicit(1, Type::Bool(None)));
        assert_eq!(sc.get("__shouldReport"  ).unwrap().clone(), Reg::Implicit(2, Type::Bool(None)));
        assert_eq!(sc.get("Micros"          ).unwrap().clone(), Reg::Implicit(3, Type::Num(None)));
        assert_eq!(sc.get("Cwnd"            ).unwrap().clone(), Reg::Implicit(4, Type::Num(None)));
        assert_eq!(sc.get("Rate"            ).unwrap().clone(), Reg::Implicit(5, Type::Num(None)));

        // state
        assert_eq!(sc.get("Report.foo").unwrap().clone(), Reg::Report(0, Type::Num(Some(0)), false));
    }

    #[test]
    fn reg() {
        let foo = b"
        (def (Report.foo 0))
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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__eventFlag").unwrap().clone(),
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
        (def (Report.foo 0))
        (when true
            (bind Report.foo 4)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        
        // check for underscored state variable
        let foo2 = b"
        (def (Report.foo_bar 0))
        (when true
            (bind Report.foo_bar 4)
        )
        ";
        
        let (p2, mut sc2) = Prog::new_with_scope(foo2).unwrap();
        let b2 = Bin::compile_prog(&p2, &mut sc2).unwrap();
        assert_eq!(b2, b);
    }

    #[test]
    fn ewma() {
        let foo = b"
        (def (Report.foo 0))
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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__eventFlag").unwrap().clone(),
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
        (def (Report.foo +infinity))
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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__eventFlag").unwrap().clone(),
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
    fn control_def() {
        let foo = b"
        (def (Control.foo +infinity))
        (when (< Flow.rtt_sample_us Control.foo)
            (bind Control.foo Flow.rtt_sample_us)
            (report)
        )
        ";

        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let foo_reg = sc.get("Control.foo").unwrap().clone();

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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Lt,
                        left: sc.get("Flow.rtt_sample_us").unwrap().clone(),
                        right: foo_reg.clone(),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: sc.get("Flow.rtt_sample_us").unwrap().clone(),
                    },
                    Instr {
                        res: sc.get("__shouldReport").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__shouldReport").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                ]
            }
        );
    }

    #[test]
    fn intermediate() {
        let foo = b"
        (def (Report.foo 0))
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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__eventFlag").unwrap().clone(),
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
        (def (Report.foo 0))
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
                        res: sc.get("__eventFlag").unwrap().clone(),
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
    fn bool_ops() {
        let foo =  b" 
		(def (Report.acked 0) (Control.state 0))
		(when true
			(:= Report.acked (+ Report.acked Ack.bytes_acked))
			(fallthrough)
		)
		(when (&& (> Micros 3000000) (== Control.state 0))
			(:= Control.state 1)
			(report)
		)
		";
        
        let (p, mut sc) = Prog::new_with_scope(foo).unwrap();
        let b = Bin::compile_prog(&p, &mut sc).unwrap();
        let evflag_reg = sc.get("__eventFlag").unwrap().clone();
        let continue_reg = sc.get("__shouldContinue").unwrap().clone();
        let acked_reg = sc.get("Report.acked").unwrap().clone();
        let state_reg = sc.get("Control.state").unwrap().clone();

        assert_eq!(
            b,
            Bin{ 
                events: vec![
                    Event { flag_idx: 2, num_flag_instrs: 1, body_idx: 3, num_body_instrs: 3 }, 
                    Event { flag_idx: 6, num_flag_instrs: 3, body_idx: 9, num_body_instrs: 2 }
                ], 
                instrs: vec![
                    Instr { 
                        res: state_reg.clone(), 
                        op: Op::Def, 
                        left: state_reg.clone(), 
                        right: Reg::ImmNum(0),
                    }, 
                    Instr { 
                        res: acked_reg.clone(),
                        op: Op::Def, 
                        left: acked_reg.clone(),
                        right: Reg::ImmNum(0),
                    }, 
                    Instr { 
                        res: evflag_reg.clone(),
                        op: Op::Bind,
                        left: evflag_reg.clone(),
                        right: Reg::ImmBool(true),
                    }, 
                    Instr { 
                        res: Reg::Tmp(0, Type::Num(None)),
                        op: Op::Add,
                        left: acked_reg.clone(),
                        right: sc.get("Ack.bytes_acked").unwrap().clone(),
                    }, 
                    Instr { 
                        res: acked_reg.clone(),
                        op: Op::Bind,
                        left: acked_reg.clone(),
                        right: Reg::Tmp(0, Type::Num(None)),
                    }, 
                    Instr { 
                        res: continue_reg.clone(),
                        op: Op::Bind,
                        left: continue_reg.clone(),
                        right: Reg::ImmBool(true),
                    }, 
                    Instr { 
                        res: Reg::Tmp(0, Type::Bool(None)),
                        op: Op::Gt,
                        left: sc.get("Micros").unwrap().clone(),
                        right: Reg::ImmNum(3000000) 
                    }, 
                    Instr { 
                        res: Reg::Tmp(1, Type::Bool(None)),
                        op: Op::Equiv,
                        left: state_reg.clone(),
                        right: Reg::ImmNum(0) 
                    }, 
                    Instr { 
                        res: evflag_reg.clone(),
                        op: Op::Mul,
                        left: Reg::Tmp(0, Type::Bool(None)),
                        right: Reg::Tmp(1, Type::Bool(None)) 
                    }, 
                    Instr { 
                        res: state_reg.clone(),
                        op: Op::Bind,
                        left: state_reg.clone(),
                        right: Reg::ImmNum(1) 
                    }, 
                    Instr { 
                        res: sc.get("__shouldReport").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__shouldReport").unwrap().clone(),
                        right: Reg::ImmBool(true) 
                    },
                ],
            },
        );
    }
    
    #[test]
    fn multiple_events() {
        let foo = b"
        (def (Report.foo 0))
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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(4),
                    },
                    Instr {
                        res: sc.get("__eventFlag").unwrap().clone(),
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

    #[test]
    fn commands() {
        let foo = b"
        (def (Report.foo 0))
        (when true
            (bind Report.foo 4)
            (fallthrough)
        )
        (when (> Micros 3000)
            (bind Report.foo 5)
            (report)
            (:= Micros 0)
        )";

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
                        num_body_instrs: 2,
                    },
                    Event{
                        flag_idx: 4,
                        num_flag_instrs: 1,
                        body_idx: 5,
                        num_body_instrs: 3,
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
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__eventFlag").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(4),
                    },
                    Instr {
                        res: sc.get("__shouldContinue").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__shouldContinue").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: sc.get("__eventFlag").unwrap().clone(),
                        op: Op::Gt,
                        left: sc.get("Micros").unwrap().clone(),
                        right: Reg::ImmNum(3000),
                    },
                    Instr {
                        res: foo_reg.clone(),
                        op: Op::Bind,
                        left: foo_reg.clone(),
                        right: Reg::ImmNum(5),
                    },
                    Instr {
                        res: sc.get("__shouldReport").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("__shouldReport").unwrap().clone(),
                        right: Reg::ImmBool(true),
                    },
                    Instr {
                        res: sc.get("Micros").unwrap().clone(),
                        op: Op::Bind,
                        left: sc.get("Micros").unwrap().clone(),
                        right: Reg::ImmNum(0),
                    },
                ]
            }
        );
    }
}
