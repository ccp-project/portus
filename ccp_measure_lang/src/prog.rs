use super::{Error, Result};
use super::ast::{Expr, atom};
use super::datapath::{Type, check_atom_type};
use super::scope::Scope;
use std::str;

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
            map_res!(atom, |a: Result<Expr>| a.and_then(|i| check_atom_type(&i)))
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
                        x@ Type::Num(_) | x@ Type::Bool(_) => (name, x),
                        _ => (name, Type::None)
                    } 
                }).collect()
            )
        ),
        tag!(")")
    ))
);

impl Prog {
    pub fn new_with_scope(source: &[u8]) -> Result<(Self, Scope)> {
        let mut scope = Scope::new();
        use nom::{IResult, Needed};
        let rest = match defs(source) {
            IResult::Done(rest, flow_state) => {
                flow_state
                    .into_iter()
                    .map(|(var, typ)| match var {
                        Type::Name(v) => (format!("Flow.{}", v), typ),
                        _ => unreachable!(),
                    })
                    .for_each(|(var, typ)| { scope.new_perm(var, typ); });

                Ok(rest)
            }
            IResult::Error(e) => Err(Error::from(e)),
            IResult::Incomplete(Needed::Unknown) => Err(Error::from(String::from("need more src"))),
            IResult::Incomplete(Needed::Size(s)) => Err(
                Error::from(format!("need {} more bytes", s)),
            ),
        }?;

        // TODO make Expr::new return Iter, make self wrap an iter also
        Ok((Prog(Expr::new(rest)?), scope))
    }

    pub fn new(source: &[u8]) -> Result<Self> {
        Self::new_with_scope(source).map(|t| t.0)
    }
}

#[cfg(test)]
mod test {
    use datapath::Type;

    #[test]
    fn defs() {
        let foo = b"(def (Foo 0) (Bar 0) (Baz 0))";
        use nom::{IResult, Needed};
        match super::defs(foo) {
            IResult::Done(r, me) => {
                assert_eq!(r, &[]);
                assert_eq!(
                me,
                vec![
                    (Type::Name(String::from("Foo")), Type::Num(Some(0))),
                    (Type::Name(String::from("Bar")), Type::Num(Some(0))),
                    (Type::Name(String::from("Baz")), Type::Num(Some(0))),
                ]
            );
            }
            IResult::Error(e) => panic!(e),
            IResult::Incomplete(Needed::Unknown) => panic!("incomplete"),
            IResult::Incomplete(Needed::Size(s)) => panic!("need {} more bytes", s),
        }
    }
}
