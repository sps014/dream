//! LLVM first-class types as they appear in textual IR.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Void,
    Int(u32),
    F32,
    F64,
    /// Opaque pointer (`ptr`, address space 0).
    Ptr,
    /// A module-level named struct (`%name`), declared through `ModuleWriter::named_struct`.
    Named(String),
    Array(u64, Box<Ty>),
    Vector(u32, Box<Ty>),
    Struct {
        packed: bool,
        fields: Vec<Ty>,
    },
}

impl Ty {
    pub const I1: Ty = Ty::Int(1);
    pub const I8: Ty = Ty::Int(8);
    pub const I16: Ty = Ty::Int(16);
    pub const I32: Ty = Ty::Int(32);
    pub const I64: Ty = Ty::Int(64);

    pub fn bytes(n: u64) -> Ty {
        Ty::Array(n, Box::new(Ty::I8))
    }

    pub fn is_int(&self) -> bool {
        matches!(self, Ty::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Ty::F32 | Ty::F64)
    }

    pub fn int_bits(&self) -> Option<u32> {
        match self {
            Ty::Int(b) => Some(*b),
            _ => None,
        }
    }

    pub fn is_void(&self) -> bool {
        matches!(self, Ty::Void)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Void => f.write_str("void"),
            Ty::Int(b) => write!(f, "i{b}"),
            Ty::F32 => f.write_str("float"),
            Ty::F64 => f.write_str("double"),
            Ty::Ptr => f.write_str("ptr"),
            Ty::Named(n) => write!(f, "%{}", super::fmt::ident(n)),
            Ty::Array(n, e) => write!(f, "[{n} x {e}]"),
            Ty::Vector(n, e) => write!(f, "<{n} x {e}>"),
            Ty::Struct { packed, fields } => {
                if *packed {
                    f.write_str("<")?;
                }
                f.write_str("{ ")?;
                for (i, t) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{t}")?;
                }
                f.write_str(" }")?;
                if *packed {
                    f.write_str(">")?;
                }
                Ok(())
            }
        }
    }
}

/// A function type: `ret (params...)`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FnTy {
    pub ret: Ty,
    pub params: Vec<Ty>,
    pub varargs: bool,
}

impl FnTy {
    pub fn new(ret: Ty, params: Vec<Ty>) -> Self {
        Self {
            ret,
            params,
            varargs: false,
        }
    }
}

impl fmt::Display for FnTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (", self.ret)?;
        for (i, p) in self.params.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{p}")?;
        }
        if self.varargs {
            if !self.params.is_empty() {
                f.write_str(", ")?;
            }
            f.write_str("...")?;
        }
        f.write_str(")")
    }
}
