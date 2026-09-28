//! Typed SSA operands.
//!
//! There is deliberately no `undef`/`poison` constructor: the printer cannot introduce either,
//! so any such value in the output came from LLVM's own passes.

use super::fmt;
use super::ty::Ty;

#[derive(Clone, Debug, PartialEq)]
pub enum Repr {
    /// `%vN`, an instruction result.
    Reg(u32),
    /// `%aN`, the N-th function parameter.
    Arg(u32),
    /// `@name`.
    Global(String),
    Int(i128),
    /// IEEE bit pattern, printed via `fmt::f64_lit` (floats are widened first).
    Float(f64),
    Null,
    Zero,
    /// A constant expression printed verbatim (e.g. `getelementptr (i8, ptr @s, i64 16)`).
    ConstExpr(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub ty: Ty,
    pub repr: Repr,
}

impl Value {
    pub fn new(ty: Ty, repr: Repr) -> Self {
        Self { ty, repr }
    }

    pub fn int(ty: Ty, v: i128) -> Self {
        Self::new(ty, Repr::Int(v))
    }

    pub fn i1(b: bool) -> Self {
        Self::int(Ty::I1, b as i128)
    }

    pub fn i32(v: i64) -> Self {
        Self::int(Ty::I32, v as i128)
    }

    pub fn i64(v: i64) -> Self {
        Self::int(Ty::I64, v as i128)
    }

    pub fn f64(v: f64) -> Self {
        Self::new(Ty::F64, Repr::Float(v))
    }

    pub fn f32(v: f32) -> Self {
        Self::new(Ty::F32, Repr::Float(v as f64))
    }

    pub fn null() -> Self {
        Self::new(Ty::Ptr, Repr::Null)
    }

    pub fn zero(ty: Ty) -> Self {
        let repr = match &ty {
            Ty::Int(_) => Repr::Int(0),
            Ty::F32 | Ty::F64 => Repr::Float(0.0),
            Ty::Ptr => Repr::Null,
            _ => Repr::Zero,
        };
        Self::new(ty, repr)
    }

    pub fn global(name: impl Into<String>) -> Self {
        Self::new(Ty::Ptr, Repr::Global(name.into()))
    }

    pub fn const_int(&self) -> Option<i128> {
        match self.repr {
            Repr::Int(v) => Some(v),
            _ => None,
        }
    }

    pub fn is_const(&self) -> bool {
        !matches!(self.repr, Repr::Reg(_) | Repr::Arg(_))
    }

    /// The operand without its type (`%v3`, `42`, `@g`).
    pub fn operand(&self) -> String {
        match &self.repr {
            Repr::Reg(n) => format!("%v{n}"),
            Repr::Arg(n) => format!("%a{n}"),
            Repr::Global(g) => fmt::global(g),
            Repr::Int(v) => {
                if self.ty == Ty::I1 {
                    if *v & 1 == 1 { "true" } else { "false" }.to_string()
                } else {
                    v.to_string()
                }
            }
            Repr::Float(v) => fmt::f64_lit(*v),
            Repr::Null => "null".into(),
            Repr::Zero => "zeroinitializer".into(),
            Repr::ConstExpr(e) => e.clone(),
        }
    }

    /// `ty operand`, the form every instruction operand takes.
    pub fn typed(&self) -> String {
        format!("{} {}", self.ty, self.operand())
    }
}
