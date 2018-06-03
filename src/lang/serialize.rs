use super::{Error, Result};
use super::ast::Op;
use super::datapath::{Bin, Event, Instr, Reg};
use ::serialize::u32_to_u8s;

/// Serialize a Bin to bytes for transfer to the datapath
impl Bin {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let b = self.clone();
        let ists = b.instrs
            .into_iter()
            .flat_map(|i| i.into_iter());
        b.events
            .into_iter()
            .flat_map(|i| i.into_iter())
            .chain(ists)
            .collect()
    }
}
/// pub struct Event {
///     flag_idx: u32,
///     num_flag_instrs: u32,
///     body_idx: u32,
///     num_body_instrs: u32
/// }
/// Emit each of the four fields of `datapath::Event` as a `u32`. This implies
/// that the maximum number of instructions is 1024, but we limit this value in libccp.
/// The number of flag instructions is further limited by the maximum expression
/// depth (the number of temporary registers), which is 8.
///
/// serialization format:
///
/// |----------------|-----------------|----------------|-----------------|
/// | flag instr idx | num flag instrs | body instr idx | num body instrs |
/// | u32            | u32             | u32            | u32             |
/// |----------------|-----------------|----------------|-----------------|
impl IntoIterator for Event {
    type Item = Result<u8>;
    type IntoIter = ::std::vec::IntoIter<Result<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        let v = &mut[0, 0, 0, 0,
                     0, 0, 0, 0,
                     0, 0, 0, 0,
                     0, 0, 0, 0];
        u32_to_u8s(&mut v[0..=3], self.flag_idx);
        u32_to_u8s(&mut v[4..=7], self.num_flag_instrs);
        u32_to_u8s(&mut v[8..=11], self.body_idx);
        u32_to_u8s(&mut v[12..=15], self.num_body_instrs);
        v.iter().map(|u| Ok(*u)).collect::<Vec<Result<u8>>>().into_iter()
    }
}

/// pub struct Instr {
///     res: Reg,
///     op: Op,
///     left: Reg,
///     right: Reg,
/// }
///
/// serialization format: (16 B)
/// |---------|------------|---------------|----------|-------------|----------|--------------|
/// |Opcode   |Result Type |Result Register|Left Type |Left Register|Right Type|Right Register|
/// |u8       |u8          |u32            |u8        |u32          |u8        |u32           |
/// |---------|------------|---------------|----------|-------------|----------|--------------|
impl IntoIterator for Instr {
    type Item = Result<u8>;
    type IntoIter = ::std::vec::IntoIter<Result<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        let op = vec![Ok(serialize_op(&self.op))];
        op.into_iter()
            .chain(self.res)
            .chain(self.left)
            .chain(self.right)
            .collect::<Vec<Result<u8>>>() // annoying that this collect is necessary, otherwise Self::IntoIter is unreadable
            .into_iter()
    }
}

fn serialize_op(o: &Op) -> u8 {
    match *o {
        Op::Add      => 0,
        Op::And      => unreachable!(),
        Op::Bind     => 1,
        Op::Def      => 2,
        Op::Div      => 3,
        Op::Equiv    => 4,
        Op::Ewma     => 5,
        Op::Gt       => 6,
        Op::If       => 7,
        Op::Lt       => 8,
        Op::Max      => 9,
        Op::MaxWrap  => 10,
        Op::Min      => 11,
        Op::Mul      => 12,
        Op::NotIf    => 13,
        Op::Or       => unreachable!(),
        Op::Sub      => 14,
    }
}

impl IntoIterator for Reg {
    type Item = Result<u8>;
    type IntoIter = ::std::vec::IntoIter<Result<u8>>;

    fn into_iter(self) -> Self::IntoIter {
        let reg = match self {
            Reg::Control(i, _) => {
                if i > 15 {
                    Err(Error::from(
                        format!("Control Register index too big (max 15): {:?}", i),
                    ))
                } else {
                    Ok((0u8, u32::from(i)))
                }
            }
            Reg::ImmBool(bl) => Ok((1u8, bl as u32)),
            Reg::ImmNum(num) => {
                if num == u64::max_value() || num < (1 << 31) {
                    Ok((1u8, num as u32))
                } else {
                    Err(Error::from(
                        format!("ImmNum too big (max 32 bits): {:?}", num),
                    ))
                }
            }
            Reg::Implicit(i, _) => {
                if i > 5 {
                    Err(Error::from(
                        format!("Implicit Register index too big (max 5): {:?}", i),
                    ))
                } else {
                    Ok((2u8, u32::from(i)))
                }
            }
            Reg::Local(i, _) => {
                if i > 5 {
                    Err(Error::from(
                        format!("Local Register index too big (max 5): {:?}", i),
                    ))
                } else {
                    Ok((3u8, u32::from(i)))
                }
            }
            Reg::Primitive(i, _) => {
                if i > 15 {
                    Err(Error::from(
                        format!("Primitive Register index too big (max 15): {:?}", i),
                    ))
                } else {
                    Ok((4u8, u32::from(i)))
                }
            }
            Reg::Report(i, _, is_volatile) => {
                if i > 15 {
                    Err(Error::from(
                        format!("Report Register index too big (max 15): {:?}", i),
                    ))
                } else {
                    // in libccp:
                    // VOLATILE_REPORT_REG is type #5
                    // NONVOLATILE_REPORT_REG is typ #6
                    // so, here, we differentiate between variables marked by the volatile keyword.
                    Ok((if is_volatile { 5u8 } else { 6u8 }, u32::from(i)))
                }
            }
            Reg::Tmp(i, _) => {
                if i > 15 {
                    Err(Error::from(
                        format!("Tmp Register index too big (max 15): {:?}", i),
                    ))
                } else {
                    Ok((7u8, u32::from(i)))
                }
            }
            Reg::None => unreachable!(),
        };

        reg
            .map(|(typ, idx)| {
                let v = &mut[typ, 0, 0, 0, 0];
                u32_to_u8s(&mut v[1..5], idx);
                v.iter().map(|u| Ok(*u)).collect::<Vec<Result<u8>>>().into_iter()
            })
            .unwrap_or_else(|e| vec![Err(e)].into_iter())
    }
}

impl Reg {
    pub fn deserialize(_buf: &[u8]) -> Self {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use lang;
    use lang::ast::Op;
    use lang::datapath::{Bin, Event, Instr, Type, Reg};
    #[test]
    fn do_ser() {
        // make a Bin to serialize
        let b = Bin{
            events: vec![Event{
                flag_idx: 1,
                num_flag_instrs: 1,
                body_idx: 2,
                num_body_instrs: 1,
            }],
            instrs: vec![
                Instr {
                    res: Reg::Report(6, Type::Num(Some(0)), true),
                    op: Op::Def,
                    left: Reg::Report(6, Type::Num(Some(0)), true),
                    right: Reg::ImmNum(0),
                },
                Instr {
                    res: Reg::Implicit(0, Type::Bool(None)),
                    op: Op::Bind,
                    left: Reg::Implicit(0, Type::Bool(None)),
                    right: Reg::ImmBool(true),
                },
                Instr {
                    res: Reg::Report(6, Type::Num(Some(0)), true),
                    op: Op::Bind,
                    left: Reg::Report(6, Type::Num(Some(0)), true),
                    right: Reg::ImmNum(4),
                },
            ]
        };

        let v = b.serialize().expect("serialize");
        assert_eq!(
            v,
            vec![
                // event description
                0x01, 0x00, 0x00, 0x00,
                0x01, 0x00, 0x00, 0x00,
                0x02, 0x00, 0x00, 0x00,
                0x01, 0x00, 0x00, 0x00,
                // def reg::report(6) <- 0
                0x02, 0x05, 0x06, 0x00, 0x00, 0x00, 0x05, 0x06, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 
                // reg::eventFlag <- 1
                0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00,
                // def reg::report(6) <- 0
                0x01, 0x05, 0x06, 0x00, 0x00, 0x00, 0x05, 0x06, 0x00, 0x00, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 
            ]
        );
    }
    
    #[test]
    fn do_ser_max_imm() {
        // make an InstrBytes to serialize
        let b = Instr {
            res: Reg::Tmp(0, Type::Num(None)),
            op: Op::Add,
            left: Reg::ImmNum(0x3fff_ffff),
            right: Reg::ImmNum(0x3fff_ffff),
        };

        let v = b.into_iter().collect::<lang::Result<Vec<u8>>>().expect("serialize");
        assert_eq!(
            v,
            vec![ 
                0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x01, 0xff, 0xff, 0xff, 0x3f, 0x01, 0xff, 0xff, 0xff, 0x3f, 
            ]
        );
    }
    
    #[test]
    fn do_ser_def_max_imm() {
        // make a Bin to serialize
        let b = Instr {
            res: Reg::Report(2, Type::Num(Some(u64::max_value())), true),
            op: Op::Def,
            left: Reg::Report(2, Type::Num(Some(u64::max_value())), true),
            right: Reg::ImmNum(u64::max_value()),
        };

        let v = b.into_iter().collect::<lang::Result<Vec<u8>>>().expect("serialize");
        assert_eq!(
            v,
            vec![
                0x02, 0x05, 0x02, 0x00, 0x00, 0x00, 0x05, 0x02, 0x00, 0x00, 0x00, 0x01, 0xff, 0xff, 0xff, 0xff,
            ]
        );
    }
}
