use std::iter::Iterator;
use super::{Error, Result};
use super::ast::Op;
use super::datapath::{Bin, Instr, Reg};

/// Serialize a Bin to bytes for transfer to the datapath
pub(crate) fn serialize(b: Bin) -> Result<Vec<u8>> {
    b.into_iter().flat_map(|i| i.into_iter()).collect()
}

impl IntoIterator for Instr {
    type Item = Result<u8>;
    type IntoIter = instrBytes;

    fn into_iter(self) -> Self::IntoIter {
        instrBytes { i: self, which: 0 }
    }
}

pub(crate) struct instrBytes {
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
/// |u8              |u8:typ2,which6  |u8:typ2,which6  |u8:typ2,which6  |
/// |----------------|----------------|----------------|----------------|
impl Iterator for instrBytes {
    type Item = Result<u8>;

    /// Yield the bytes of this instruction
    fn next(&mut self) -> Option<Result<u8>> {
        self.which += 1;
        match self.which {
            1 => Some(Ok(serialize_op(&self.i.op))),
            2 => Some(serialize_reg(&self.i.res)),
            3 => Some(serialize_reg(&self.i.left)),
            4 => Some(serialize_reg(&self.i.right)),
            _ => None,
        }
    }
}

fn serialize_op(o: &Op) -> u8 {
    match o {
        &Op::Add => 0,
        &Op::Bind => 1,
        &Op::Div => 2,
        &Op::Equiv => 3,
        &Op::Ewma => 4,
        &Op::Gt => 5,
        &Op::If => 6,
        &Op::Let => 7,
        &Op::Lt => 8,
        &Op::Max => 9,
        &Op::Min => 10,
        &Op::Mul => 11,
        &Op::NotIf => 12,
        &Op::Sub => 13,
    }
}

fn serialize_reg(r: &Reg) -> Result<u8> {
    match r {
        &Reg::ImmNum(num) => {
            if num > (1 << 6) {
                Err(Error::from(
                    format!("ImmNum too big (max 6 bits): {:?}", num),
                ))
            } else {
                Ok((num & 0b00111111) as u8)
            }
        }
        &Reg::ImmBool(bl) => Ok((0b00000001 & (bl as u8)) as u8),
        &Reg::Const(i, _) => {
            let reg = if i > (1 << 6) {
                return Err(Error::from(format!(
                    "Const Register index too big (max 6 bits): {:?}",
                    i
                )));
            } else {
                i | 0b11000000
            };

            Ok((reg & 0b01111111) as u8)
        }
        &Reg::Tmp(i, _) => {
            let reg = if i > (1 << 6) {
                return Err(Error::from(
                    format!("Tmp Register index too big (max 6 bits): {:?}", i),
                ));
            } else {
                i | 0b11000000
            };

            Ok((reg & 0b10111111) as u8)
        }
        &Reg::Perm(i, _) => {
            let reg = if i > (1 << 6) {
                return Err(Error::from(
                    format!("Perm Register index too big (max 6 bits): {:?}", i),
                ));
            } else {
                i | 0b11000000
            };

            Ok((reg & 0b11111111) as u8)
        }
        &Reg::None => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use ast::Op;
    use datapath::{Bin, Instr, Type, Reg};
    use super::serialize;
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

        let v = serialize(b).expect("serialize");
        assert_eq!(
            v,
            vec![
                0, 128, 2, 3,    // add instr
                1, 192, 192, 128 // bind instr
            ]
        );
    }
}
