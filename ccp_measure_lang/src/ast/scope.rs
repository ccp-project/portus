use std::collections::{HashMap, HashSet};

use super::Type;

#[derive(Debug)]
pub(crate) struct Scope(HashMap<Type, Type>, HashSet<Type>);

impl Scope {
    pub fn with(vars: Vec<(Type, Type)>) -> Self {
        let mut hm = HashMap::new();
        for (name, val) in vars {
            hm.insert(name, val);
        }

        let hs = HashSet::new();
        Scope(hm, hs)
    }

    /// Define variables always accessible in the datapath,
    /// in the context of the most recent packet.
    /// All datapaths shall recognize these Names.
    pub fn datapath_scope() -> Self {
        Self::with(vec![
            (Type::Name(String::from("Ack")), Type::Num),
            (Type::Name(String::from("Rtt")), Type::Num),
            (Type::Name(String::from("SndRate")), Type::Num),
            (Type::Name(String::from("RcvRate")), Type::Num),
        ])
    }

    pub fn get(&self, t: &Type) -> Option<&Type> {
        self.0.get(t)
    }

    pub fn init(&mut self, t: Type, v: Type) -> Option<Type> {
        self.1.insert(t.clone());
        self.0.insert(t, v)
    }

    pub fn bind(&mut self, t: Type, v: Type) -> Option<Type> {
        if self.1.remove(&t.clone()) {
            self.0.insert(t, v);
            None
        } else {
            self.0.insert(t, v)
        }
    }
}
