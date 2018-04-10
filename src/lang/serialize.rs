use super::{Error, Result};
use super::ast::Op;
use super::datapath::{Bin, Instr, Reg};
use ::serialize::u32_to_u8s;

/// Serialize a Bin to bytes for transfer to the datapath
impl Bin {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        self.clone()
            .into_iter()
            .flat_map(|i| i.into_iter())
            .collect()
    }
}

impl IntoIterator for Instr {
    type Item = Result<u8>;
    type IntoIter = InstrBytes;

    fn into_iter(self) -> Self::IntoIter {
        InstrBytes { i: self, which: 0 }
    }
}

pub struct InstrBytes {
    i: Instr,
    which: u8,
}

/// pub struct Instr {
///     res: Reg,
///     op: Op,
///     left: Reg,
///     right: Reg,
/// }
///
/// serialization format:
/// |----------------|----------------|----------------|----------------|
/// |Opcode          |Result Register |Left Register   |Right Register  |
/// |u8              |u8:typ2,which6  |u32:typ2,which30|u32:typ2,which30|
/// |----------------|----------------|----------------|----------------|
impl Iterator for InstrBytes {
    type Item = Result<u8>;

    /// Yield the bytes of this instruction
    fn next(&mut self) -> Option<Result<u8>> {
        self.which += 1;
        let reg = match self.which {
            0 => unreachable!(),
            1 => { return Some(Ok(serialize_op(&self.i.op))); }
            2 => { return Some(serialize_reg_u8(&self.i.res)); }
            3 | 4 | 5 | 6 => serialize_reg_u32(&self.i.left),
            7 | 8 | 9 | 10 => serialize_reg_u32(&self.i.right),
            _ => {return None},
        };

        if reg.is_err() {
            return Some(reg.map(|_| unreachable!()));
        }
                
        let mut buf = [0u8; 4];
        u32_to_u8s(&mut buf, reg.unwrap());
        Some(Ok(buf[((self.which - 3) % 4) as usize]))
    }
}

fn serialize_op(o: &Op) -> u8 {
    match *o {
        Op::Add => 0,
        Op::Bind => 1,
        Op::Def => 14, // TODO fix the numbers
        Op::Div => 2,
        Op::Equiv => 3,
        Op::Ewma => 4,
        Op::Gt => 5,
        Op::If => 6,
        Op::Lt => 8,
        Op::Max => 9,
        Op::MaxWrap => 15,
        Op::Min => 10,
        Op::Mul => 11,
        Op::NotIf => 12,
        Op::Sub => 13,
    }
}

fn serialize_reg_u8(r: &Reg) -> Result<u8> {
    match *r {
        Reg::ImmNum(_) | Reg::ImmBool(_) => Err(Error::from("Cannot fit immediate in 8 bit register")),
        Reg::Const(i, _) => {
            let reg = if i > (1 << 6) {
                return Err(Error::from(format!(
                    "Const Register index too big (max 6 bits): {:?}",
                    i
                )));
            } else {
                i | 0b1100_0000
            };

            Ok((reg & 0b0111_1111) as u8)
        }
        Reg::Tmp(i, _) => {
            let reg = if i > (1 << 6) {
                return Err(Error::from(
                    format!("Tmp Register index too big (max 6 bits): {:?}", i),
                ));
            } else {
                i | 0b1100_0000
            };

            Ok((reg & 0b1011_1111) as u8)
        }
        Reg::Perm(i, _) => {
            let reg = if i > (1 << 6) {
                return Err(Error::from(
                    format!("Perm Register index too big (max 6 bits): {:?}", i),
                ));
            } else {
                i | 0b1100_0000
            };

            Ok(reg as u8)
        }
        Reg::None => unreachable!(),
    }
}

fn serialize_reg_u32(r: &Reg) -> Result<u32> {
    match *r {
        Reg::ImmNum(num) => {
            if num == u64::max_value() || num < (1 << 31) {
                Ok(num as u32 & 0x3fff_ffff)
            } else {
                Err(Error::from(
                    format!("ImmNum too big (max 30 bits): {:?}", num),
                ))
            }
        }
        Reg::ImmBool(bl) => Ok(bl as u32),
        Reg::Const(i, _) => {
            let reg = if i > 15 {
                return Err(Error::from(format!(
                    "Const Register index too big (max 15): {:?}",
                    i
                )));
            } else {
                u32::from(i) | 0xc000_0000
            };

            Ok((reg & 0x7fff_ffff) as u32)
        }
        Reg::Tmp(i, _) => {
            let reg = if i > 15 {
                return Err(Error::from(
                    format!("Tmp Register index too big (max 15): {:?}", i),
                ));
            } else {
                u32::from(i) | 0xc000_0000
            };

            Ok((reg & 0xbfff_ffff) as u32)
        }
        Reg::Perm(i, _) => {
            let reg = if i > 15 {
                return Err(Error::from(
                    format!("Perm Register index too big (max 15): {:?}", i),
                ));
            } else {
                u32::from(i) | 0xc000_0000
            };

            Ok(reg as u32)
        }
        Reg::None => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use lang::ast::Op;
    use lang::datapath::{Bin, Instr, Type, Reg};
    #[test]
    fn do_ser() {
        // make a Bin to serialize
        let b = Bin(vec![
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
        ]);

        let v = b.serialize().expect("serialize");
        assert_eq!(
            v,
            vec![
                0, 0x80, 2, 0, 0, 0, 3, 0, 0, 0,    // add instr
                1, 0xc0, 0, 0, 0, 0xc0, 0, 0, 0, 0x80 // bind instr
            ]
        );
    }
    
    #[test]
    fn do_ser_max_imm() {
        // make a Bin to serialize
        let b = Bin(vec![
            Instr {
                res: Reg::Tmp(0, Type::Num(None)),
                op: Op::Add,
                left: Reg::ImmNum(0x3fff_ffff),
                right: Reg::ImmNum(0x3fff_ffff),
            },
        ]);

        let v = b.serialize().expect("serialize");
        assert_eq!(
            v,
            vec![
                0, 0x80, 0xff, 0xff, 0xff, 0x3f, 0xff, 0xff, 0xff, 0x3f,    // add instr
            ]
        );
    }
    
    #[test]
    fn do_ser_def_max_imm() {
        // make a Bin to serialize
        let b = Bin(vec![
            Instr {
                res: Reg::Perm(2, Type::Num(Some(u64::max_value()))),
                op: Op::Def,
                left: Reg::Perm(2, Type::Num(Some(u64::max_value()))),
                right: Reg::ImmNum(u64::max_value()),
            },
        ]);

        let v = b.serialize().expect("serialize");
        assert_eq!(
            v,
            vec![
                14, 0xc2, 2, 0, 0, 0xc0, 0xff, 0xff, 0xff, 0x3f,    // def instr
            ]
        );
    }
}
