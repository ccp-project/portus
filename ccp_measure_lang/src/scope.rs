use std::collections::HashMap;
use super::datapath::{Reg, Type};

pub struct Scope {
    named: HashMap<String, Reg>,
    prim: Vec<Reg>,
    perm: Vec<Reg>,
    tmp: Vec<Reg>,
}

impl Scope {
    /// Define variables always accessible in the datapath,
    /// in the context of the most recent packet.
    /// All datapaths shall recognize these Names.
    pub fn new() -> Self {
        let mut sc = Scope {
            prim: vec![
                Reg::Const(0, Type::Num(None)),
                Reg::Const(1, Type::Num(None)),
                Reg::Const(2, Type::Num(None)),
                Reg::Const(3, Type::Num(None)),
            ],
            tmp: vec![],
            perm: vec![],
            named: HashMap::new(),
        };

        sc.named.insert(String::from("Rtt"), sc.prim[0].clone());
        sc.named.insert(String::from("Ack"), sc.prim[1].clone());
        sc.named.insert(String::from("SndRate"), sc.prim[2].clone());
        sc.named.insert(String::from("RcvRate"), sc.prim[3].clone());

        sc
    }

    pub fn get(&self, name: &String) -> Option<&Reg> {
        self.named.get(name)
    }

    pub fn new_tmp(&mut self, t: Type) -> Reg {
        let id = self.tmp.len() as u8;
        let r = Reg::Tmp(id, t);
        self.tmp.push(r);
        self.tmp[id as usize].clone()
    }

    pub fn new_perm(&mut self, name: String, t: Type) -> Reg {
        let id = self.perm.len() as u8;
        let r = Reg::Perm(id, t);
        self.perm.push(r);
        self.named.insert(name, self.perm[id as usize].clone());
        self.perm[id as usize].clone()
    }

    pub fn clear_tmps(&mut self) {
        self.tmp.clear()
    }
}
