//! The C shim every `@c` call goes through. The backend describes each crossing in Dream's own
//! register types ([`CScalar::carrier`], pointers, struct addresses); the driver renders the
//! description to C, and the shim, compiled by the pinned clang for the build target, declares
//! the real C prototype and converts. Clang therefore owns the platform ABI: struct
//! classification (`byval`/`sret`/HFA), narrow-scalar extension, and calling conventions are
//! never re-derived by hand in the backend.
//!
//! Forward shims are `always_inline`, so after `llvm-link` they disappear into the Dream
//! trampoline that calls them. Reverse adapters are real functions: C holds their address.

use dream_types::CScalar;

/// Prefix of every function and type the shim defines.
pub const SHIM_PREFIX: &str = "dream_cs_";

/// The forward shim for the `@c` import whose C-identifier form is `import`.
pub fn forward_name(import: &str) -> String {
    format!("{SHIM_PREFIX}{import}")
}

/// The getter returning the address of the C function that frees `forward`'s `OwnedCPtr` result.
pub fn destructor_getter_name(forward: &str) -> String {
    format!("{forward}__free")
}

/// The Dream function a reverse adapter named `adapter` calls.
pub fn reverse_body_name(adapter: &str) -> String {
    format!("{adapter}__body")
}

/// A C type at the boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CTy {
    Void,
    Scalar(CScalar),
    /// Any pointer (`void*`): `CPtr`, strings, arrays, callbacks, `ref` out-params.
    Ptr,
    /// A by-value struct: an index into [`CShim::structs`].
    Struct(usize),
}

/// A field of a by-value struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CField {
    Scalar(CScalar),
    Struct(usize),
}

/// A by-value struct, mirrored from a Dream @unmanaged struct. Fields keep their Dream offsets:
/// the shim copies field by field, so the C layout never has to equal Dream's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CStruct {
    pub packed: bool,
    pub fields: Vec<(u32, CField)>,
}

/// Dream calling C: Dream calls `shim`, which calls `symbol`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forward {
    pub shim: String,
    pub symbol: String,
    pub stdcall: bool,
    pub params: Vec<CTy>,
    pub ret: CTy,
}

/// C calling Dream: C calls `adapter`, which calls the Dream-emitted `body`. Only scalars and
/// pointers cross in this direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reverse {
    pub adapter: String,
    pub body: String,
    pub params: Vec<CTy>,
    pub ret: CTy,
}

/// `getter()` returns the address of the C function `symbol`, a `void (*)(void*)` destructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destructor {
    pub getter: String,
    pub symbol: String,
}

/// One program's shim.
#[derive(Debug, Default)]
pub struct CShim {
    pub structs: Vec<CStruct>,
    pub forwards: Vec<Forward>,
    pub reverses: Vec<Reverse>,
    pub destructors: Vec<Destructor>,
}

impl CShim {
    pub fn is_empty(&self) -> bool {
        self.forwards.is_empty() && self.reverses.is_empty() && self.destructors.is_empty()
    }

    /// The index of `s` in [`Self::structs`], adding it when new.
    pub fn intern_struct(&mut self, s: CStruct) -> usize {
        match self.structs.iter().position(|t| *t == s) {
            Some(i) => i,
            None => {
                self.structs.push(s);
                self.structs.len() - 1
            }
        }
    }
}
