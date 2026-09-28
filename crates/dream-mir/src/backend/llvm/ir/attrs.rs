//! Function, parameter and return attributes.
//!
//! Every attribute here is a *promise* to LLVM; callers must only attach one when Dream's
//! semantics prove it (see `docs/internals/06-llvm-backend.md` for the proof table).

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FnAttr {
    AlwaysInline,
    NoInline,
    OptNone,
    NoReturn,
    NoUnwind,
    Cold,
    WillReturn,
    /// `memory(...)`, body printed verbatim (e.g. `argmem: read`).
    Memory(String),
    /// `allockind("...")`.
    AllocKind(String),
    /// `allocsize(N)`.
    AllocSize(u32),
    /// `"alloc-family"="..."`.
    AllocFamily(String),
    /// `"target-cpu"="..."` and similar string attributes.
    Str(String, String),
    UwTable,
    FramePointerAll,
}

impl fmt::Display for FnAttr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FnAttr::AlwaysInline => f.write_str("alwaysinline"),
            FnAttr::NoInline => f.write_str("noinline"),
            FnAttr::OptNone => f.write_str("optnone"),
            FnAttr::NoReturn => f.write_str("noreturn"),
            FnAttr::NoUnwind => f.write_str("nounwind"),
            FnAttr::Cold => f.write_str("cold"),
            FnAttr::WillReturn => f.write_str("willreturn"),
            FnAttr::Memory(m) => write!(f, "memory({m})"),
            FnAttr::AllocKind(k) => write!(f, "allockind(\"{k}\")"),
            FnAttr::AllocSize(n) => write!(f, "allocsize({n})"),
            FnAttr::AllocFamily(k) => write!(f, "\"alloc-family\"=\"{k}\""),
            FnAttr::Str(k, v) => write!(f, "\"{k}\"=\"{v}\""),
            FnAttr::UwTable => f.write_str("uwtable"),
            FnAttr::FramePointerAll => f.write_str("\"frame-pointer\"=\"all\""),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ParamAttr {
    ZeroExt,
    SignExt,
    NoAlias,
    NonNull,
    NoUndef,
    NoCapture,
    ReadOnly,
    Align(u32),
    Dereferenceable(u64),
    AllocAlign,
    AllocPtr,
}

impl fmt::Display for ParamAttr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamAttr::ZeroExt => f.write_str("zeroext"),
            ParamAttr::SignExt => f.write_str("signext"),
            ParamAttr::NoAlias => f.write_str("noalias"),
            ParamAttr::NonNull => f.write_str("nonnull"),
            ParamAttr::NoUndef => f.write_str("noundef"),
            ParamAttr::NoCapture => f.write_str("captures(none)"),
            ParamAttr::ReadOnly => f.write_str("readonly"),
            ParamAttr::Align(n) => write!(f, "align {n}"),
            ParamAttr::Dereferenceable(n) => write!(f, "dereferenceable({n})"),
            ParamAttr::AllocAlign => f.write_str("allocalign"),
            ParamAttr::AllocPtr => f.write_str("allocptr"),
        }
    }
}

pub(crate) fn join<T: fmt::Display>(attrs: &[T]) -> String {
    let mut out = String::new();
    for a in attrs {
        out.push(' ');
        out.push_str(&a.to_string());
    }
    out
}

/// LLVM linkage + visibility for a definition or declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Linkage {
    External,
    Internal,
    Private,
    LinkOnceOdr,
}

impl Linkage {
    pub fn prefix(self) -> &'static str {
        match self {
            Linkage::External => "",
            Linkage::Internal => "internal ",
            Linkage::Private => "private ",
            Linkage::LinkOnceOdr => "linkonce_odr ",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum CallConv {
    #[default]
    C,
    Fast,
}

impl CallConv {
    pub fn prefix(self) -> &'static str {
        match self {
            CallConv::C => "",
            CallConv::Fast => "fastcc ",
        }
    }
}
