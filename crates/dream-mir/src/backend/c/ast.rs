//! Typed C IR. Only [`super::print`] turns this into source text.

use crate::BinOp;

#[derive(Clone, Debug, PartialEq)]
pub enum CTy {
    Void,
    U8,
    U16,
    I32,
    U32,
    Unsigned,
    I64,
    F32,
    F64,
    Ptr,
    VoidPtr,
    CharPtr,
    PtrTo(Box<CTy>),
    Array { elem: Box<CTy>, len: usize },
    Named(&'static str),
    Ident(String),
    Struct { fields: Vec<(CTy, String)> },
}

impl CTy {
    pub fn ptr_to(ty: CTy) -> CTy {
        CTy::PtrTo(Box::new(ty))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum UnOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Ident(String),
    Int(i64),
    Long(i64),
    Float(f64),
    F32(f32),
    Null,
    Nan {
        double: bool,
    },
    Inf {
        double: bool,
        neg: bool,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Ternary {
        cond: Box<Expr>,
        then_e: Box<Expr>,
        else_e: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    IndirectCall {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Cast {
        ty: CTy,
        expr: Box<Expr>,
    },
    Deref(Box<Expr>),
    AddrOf(Box<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Comma(Box<Expr>, Box<Expr>),
    PostInc(Box<Expr>),
    Compound(Vec<Expr>),
    CompoundTyped {
        ty: CTy,
        elems: Vec<Expr>,
    },
    LabelAddr(String),
    Gnu {
        stmts: Vec<Stmt>,
        result: Box<Expr>,
    },
}

impl Expr {
    pub fn id(s: impl Into<String>) -> Self {
        Expr::Ident(s.into())
    }

    pub fn local(n: u32) -> Self {
        Expr::Ident(format!("l{n}"))
    }

    pub fn global(n: u32) -> Self {
        Expr::Ident(format!("g{n}"))
    }

    pub fn i(n: i64) -> Self {
        Expr::Int(n)
    }

    pub fn call(name: impl Into<String>, args: Vec<Expr>) -> Self {
        Expr::Call {
            name: name.into(),
            args,
        }
    }

    pub fn cast(ty: CTy, expr: Expr) -> Self {
        Expr::Cast {
            ty,
            expr: Box::new(expr),
        }
    }

    pub fn deref(expr: Expr) -> Self {
        Expr::Deref(Box::new(expr))
    }

    pub fn addr_of(expr: Expr) -> Self {
        Expr::AddrOf(Box::new(expr))
    }

    pub fn unary(op: UnOp, expr: Expr) -> Self {
        Expr::Unary {
            op,
            expr: Box::new(expr),
        }
    }

    pub fn bin(op: BinOp, lhs: Expr, rhs: Expr) -> Self {
        Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    pub fn add(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::Add, lhs, rhs)
    }

    pub fn mul(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::Mul, lhs, rhs)
    }

    pub fn eq(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::Eq, lhs, rhs)
    }

    pub fn ne(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::Ne, lhs, rhs)
    }

    pub fn lt(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::Lt, lhs, rhs)
    }

    pub fn ge(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::Ge, lhs, rhs)
    }

    pub fn and(lhs: Expr, rhs: Expr) -> Self {
        Expr::bin(BinOp::And, lhs, rhs)
    }

    pub fn ternary(cond: Expr, then_e: Expr, else_e: Expr) -> Self {
        Expr::Ternary {
            cond: Box::new(cond),
            then_e: Box::new(then_e),
            else_e: Box::new(else_e),
        }
    }

    pub fn comma(a: Expr, b: Expr) -> Self {
        Expr::Comma(Box::new(a), Box::new(b))
    }

    pub fn index(base: Expr, index: Expr) -> Self {
        Expr::Index {
            base: Box::new(base),
            index: Box::new(index),
        }
    }

    pub fn dream_p(expr: Expr) -> Self {
        Expr::call("dream_p", vec![expr])
    }

    pub fn char_p(base: Expr) -> Self {
        Expr::cast(CTy::CharPtr, Expr::dream_p(base))
    }

    pub fn ptr_add(base: Expr, off: Expr) -> Self {
        Expr::add(Self::as_char_p(base), off)
    }

    fn as_char_p(base: Expr) -> Expr {
        match base {
            Expr::Cast {
                ty: CTy::CharPtr, ..
            } => base,
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
            } => Expr::Binary {
                op: BinOp::Add,
                lhs: Box::new(Self::as_char_p(*lhs)),
                rhs,
            },
            other => Expr::char_p(other),
        }
    }

    pub fn field_ptr(local: u32, off: u32) -> Self {
        Expr::ptr_add(Expr::local(local), Expr::i(off as i64))
    }

    pub fn lvalue(ty: CTy, ptr: Expr) -> Self {
        Expr::deref(Expr::cast(CTy::ptr_to(ty), ptr))
    }

    pub fn load(ty: CTy, ptr: Expr) -> Self {
        Expr::lvalue(ty, ptr)
    }

    /// True when printing this expression twice cannot re-run a call or statement-expression.
    pub fn is_dup_safe(&self) -> bool {
        !self.has_side_effect()
    }

    fn has_side_effect(&self) -> bool {
        match self {
            Expr::Call { .. }
            | Expr::IndirectCall { .. }
            | Expr::Gnu { .. }
            | Expr::PostInc(_)
            | Expr::Comma(_, _) => true,
            Expr::Cast { expr, .. }
            | Expr::Unary { expr, .. }
            | Expr::Deref(expr)
            | Expr::AddrOf(expr) => expr.has_side_effect(),
            Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs } => {
                lhs.has_side_effect() || rhs.has_side_effect()
            }
            Expr::Ternary {
                cond,
                then_e,
                else_e,
            } => cond.has_side_effect() || then_e.has_side_effect() || else_e.has_side_effect(),
            Expr::Compound(elems) | Expr::CompoundTyped { elems, .. } => {
                elems.iter().any(Expr::has_side_effect)
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum CaseKey {
    Int(i64),
    Ident(&'static str),
}

#[derive(Clone, Debug)]
pub struct SwitchArm {
    pub keys: Vec<CaseKey>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Expr(Expr),
    Assign {
        dest: Expr,
        src: Expr,
    },
    Decl {
        align: Option<u32>,
        static_: bool,
        const_: bool,
        ty: CTy,
        name: String,
        init: Option<Expr>,
    },
    If {
        cond: Expr,
        then_s: Box<Stmt>,
        else_s: Option<Box<Stmt>>,
    },
    Switch {
        expr: Expr,
        arms: Vec<SwitchArm>,
    },
    For {
        init: Box<Stmt>,
        cond: Expr,
        step: Box<Stmt>,
        body: Box<Stmt>,
    },
    Goto(String),
    GotoIndirect(Expr),
    Label(String),
    Return(Option<Expr>),
    Block(Vec<Stmt>),
    /// `#line N "path.dream"` for DWARF (from MIR `DebugLine` when `-g`).
    Line { file: String, line: u32 },
}

impl Stmt {
    pub fn expr(e: Expr) -> Stmt {
        Stmt::Expr(e)
    }

    pub fn assign(dest: Expr, src: Expr) -> Stmt {
        Stmt::Assign { dest, src }
    }

    pub fn decl(ty: CTy, name: impl Into<String>, init: Option<Expr>) -> Stmt {
        Stmt::Decl {
            align: None,
            static_: false,
            const_: false,
            ty,
            name: name.into(),
            init,
        }
    }

    pub fn if_(cond: Expr, then_s: Stmt) -> Stmt {
        Stmt::If {
            cond,
            then_s: Box::new(then_s),
            else_s: None,
        }
    }

    pub fn if_else(cond: Expr, then_s: Stmt, else_s: Stmt) -> Stmt {
        Stmt::If {
            cond,
            then_s: Box::new(then_s),
            else_s: Some(Box::new(else_s)),
        }
    }

    pub fn block(stmts: Vec<Stmt>) -> Stmt {
        Stmt::Block(stmts)
    }

    pub fn call(name: impl Into<String>, args: Vec<Expr>) -> Stmt {
        Stmt::Expr(Expr::call(name, args))
    }

    pub fn store(ty: CTy, ptr: Expr, val: Expr) -> Stmt {
        Stmt::assign(Expr::lvalue(ty.clone(), ptr), Expr::cast(ty, val))
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub ty: CTy,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Func {
    pub attr: Option<&'static str>,
    pub export: Option<String>,
    pub static_: bool,
    pub ret: CTy,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Include(&'static str),
    Global {
        thread_local: bool,
        align: Option<u32>,
        static_: bool,
        const_: bool,
        ty: CTy,
        name: String,
        init: Option<Expr>,
    },
    Proto {
        static_: bool,
        ret: CTy,
        name: String,
        params: Vec<Param>,
        import: Option<(String, String)>,
        export: Option<String>,
    },
    Func(Func),
    Typedef {
        name: String,
        ret: CTy,
        params: Vec<CTy>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Unit {
    pub items: Vec<Item>,
}
