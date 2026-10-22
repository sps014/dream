//! Byte images of all-constant primitive array literals, copied in with one `memcpy`.

use crate::{Const, Operand};
use dream_types::PrimTy;

/// The little-endian payload of `elems` stored as `prim` elements of `size` bytes, or `None` when
/// any element is not a constant of that type. Every supported target is little-endian.
pub(super) fn const_elem_bytes(prim: PrimTy, size: usize, elems: &[Operand]) -> Option<Vec<u8>> {
    if elems.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(size * elems.len());
    for elem in elems {
        let Operand::Const(value) = elem else {
            return None;
        };
        match (prim, value) {
            (
                PrimTy::Int
                | PrimTy::UInt
                | PrimTy::Long
                | PrimTy::ULong
                | PrimTy::ISize
                | PrimTy::USize
                | PrimTy::Byte,
                Const::Int(n) | Const::Long(n),
            ) if size <= 8 => out.extend_from_slice(&n.to_le_bytes()[..size]),
            (PrimTy::Bool, Const::Bool(b)) if size == 1 => out.push(u8::from(*b)),
            (PrimTy::Double, Const::Float(f)) if size == 8 => {
                out.extend_from_slice(&f.to_le_bytes())
            }
            (PrimTy::Float, Const::F32(f)) if size == 4 => out.extend_from_slice(&f.to_le_bytes()),
            (PrimTy::Float16, Const::F16(bits)) if size == 2 => {
                out.extend_from_slice(&bits.to_le_bytes())
            }
            _ => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_and_float_tables_encode_little_endian() {
        let ints = [
            Operand::Const(Const::Int(1)),
            Operand::Const(Const::Int(-2)),
        ];
        assert_eq!(
            const_elem_bytes(PrimTy::Int, 4, &ints),
            Some(vec![1, 0, 0, 0, 0xFE, 0xFF, 0xFF, 0xFF])
        );
        let doubles = [Operand::Const(Const::Float(1.0))];
        assert_eq!(
            const_elem_bytes(PrimTy::Double, 8, &doubles),
            Some(1.0f64.to_le_bytes().to_vec())
        );
        let mixed = [
            Operand::Const(Const::Int(1)),
            Operand::Const(Const::Float(1.0)),
        ];
        assert_eq!(const_elem_bytes(PrimTy::Int, 4, &mixed), None);
        assert_eq!(const_elem_bytes(PrimTy::Char, 2, &ints), None);
    }
}
