//! HIR module-level containers: program, functions, globals, interfaces, imports.

use crate::layout::LayoutTable;
use crate::nodes::{HExpr, HStmt};
use dream_types::{CScalar, DefId, TypeId};

/// A local variable slot within a function (parameters and `let`-bindings), unique per function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);

/// A module-level (global) variable slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlobalId(pub u32);

/// An index into [`Hir::instances`] identifying one monomorphized instance of a generic def.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(pub u32);

/// A whole compiled program in HIR form.
#[derive(Debug, Default)]
pub struct Hir {
    /// Non-generic functions and already-monomorphized function bodies, in emission order.
    pub functions: Vec<HFunction>,
    /// Module-level variables.
    pub globals: Vec<HGlobal>,
    /// The monomorphization worklist: each entry is a concrete `(DefId, type-args)` instance the
    /// backend must emit. Populated as type-checking discovers generic uses.
    pub instances: Vec<MonoInstance>,
    /// Memory layout (field offsets/sizes) of every nominal type, so the backend can lower
    /// field/index access to concrete loads/stores.
    pub layouts: LayoutTable,
    /// Host/extern functions the module imports. The backend emits one `(import ...)` per entry;
    /// call sites resolve to `$name` (which the import declares).
    pub imports: Vec<HImport>,
    /// `@intrinsic("key")` externs: each maps a callee `DefId` to its intrinsic key. These have no
    /// emitted body — call sites resolve directly to the runtime helper `$<key>` (e.g. `string_alloc`)
    /// or, for async intrinsics like `sleep`, are recognized by the backend and lowered to the
    /// scheduler. Recorded so the backend's symbol table can resolve the callee def.
    pub intrinsics: Vec<(DefId, String)>,
    /// Interface dispatch metadata: the ordered interfaces (index = `iface_id`) and, per
    /// implementing class, the concrete method symbol for each `(interface, slot)`. Drives the
    /// itable data + dispatch trampolines emitted by the backend, and keeps concrete interface
    /// method implementations reachable through dead-code elimination.
    pub interfaces: InterfaceTable,
    /// C-style enum members for debug decode: `TypeId` → `(enum name, [(member, disc), …])`.
    pub enums: EnumDebugTable,
    /// Source-level display name of every tagged nominal type (`Map<string, object>`, not the
    /// C-safe `Map_string_object` of [`crate::TypeLayout::name`]), for the backend's
    /// `typeof` tag router. The backend has the `TypeInterner` but no `DefTable`, so it cannot
    /// reconstruct these itself.
    pub type_names: TypeNameTable,
    /// Stable structural encodings for C callback adapters and other generated symbols.
    pub type_symbols: TypeNameTable,
    pub object_methods: indexmap::IndexMap<TypeId, ObjectMethods>,
}

/// Resolved overrides of the generated object protocol, independent of emitted names.
#[derive(Debug, Clone, Copy, Default)]
pub struct ObjectMethods {
    pub to_string: Option<DefId>,
    pub hash_code: Option<DefId>,
}

/// Source-level display names of tagged nominal types, keyed by interned `TypeId`.
pub type TypeNameTable = indexmap::IndexMap<TypeId, String>;

/// Debug metadata for C-style enums: `TypeId` → `(enum name, [(member name, discriminant), …])`.
pub type EnumDebugTable = indexmap::IndexMap<TypeId, (String, Vec<(String, i32)>)>;

/// Interface dispatch metadata carried from analysis into codegen.
#[derive(Debug, Clone, Default)]
pub struct InterfaceTable {
    /// The program's interfaces in registration order; the index into this vector is the stable
    /// `iface_id` referenced by [`HExprKind::InterfaceCall`].
    pub interfaces: Vec<InterfaceInfo>,
    /// Every class that implements at least one interface, with the concrete method symbols it
    /// supplies for each implemented interface.
    pub impls: Vec<InterfaceImpl>,
}

/// One interface's dispatch shape: its method count and the interned `fun(this, params): ret`
/// signature of each method slot (used to declare the `call_indirect` type + trampoline).
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub method_count: usize,
    /// The `call_indirect` signature (a `Func` `TypeId`) for each method slot.
    pub sigs: Vec<TypeId>,
}

/// One class's interface implementations: for each interface it implements, the concrete method
/// symbol (`{Class}_{method}`) that fills each method slot, keyed by the interface's `iface_id`.
#[derive(Debug, Clone)]
pub struct InterfaceImpl {
    /// The implementing class's interned struct type (its `struct_tags` key / runtime tag).
    pub class_ty: TypeId,
    /// `(iface_id, [concrete method symbol per slot])`.
    pub entries: Vec<(usize, Vec<String>)>,
}

/// A host function the module imports: an `extern fun` (interop) or a compiler-provided host
/// builtin (the `print_*` family). `module`/`field` name the WASM import target; `name` is the
/// internal symbol call sites reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HImport {
    /// The imported function's def, so call sites (which carry the callee `DefId`) resolve to this
    /// import's `$name` rather than the emitter's `$def{N}` fallback.
    pub def: DefId,
    pub name: String,
    pub module: String,
    pub field: String,
    pub params: Vec<TypeId>,
    /// Parallel to `params`: true for `ref` parameters (C out-params), which the WASM import
    /// receives as an `i32` address into linear memory (not the value's native WASM type).
    pub param_by_ref: Vec<bool>,
    pub ret: Option<TypeId>,
    pub is_async: bool,
    /// `@async_host` extern: on native the host takes the future as its leading argument and
    /// completes it from a foreign thread, so its poll delegates instead of blocking inline.
    pub async_host: bool,
    /// `@marshal("lpwstr")` on a `@c` extern: string args become UTF-16 rather than UTF-8.
    pub c_wide_strings: bool,
    /// `@c` only: how each parameter crosses the C boundary, parallel to `params`.
    pub c_params: Vec<CShape>,
    /// `@c` only: how the C result becomes the Dream result.
    pub c_ret: CShape,
    /// `@c_call("stdcall")`: `x86_stdcallcc` on 32-bit x86 Windows, the C convention elsewhere.
    pub c_stdcall: bool,
}

/// How one Dream value crosses the C boundary, decided (and validated) by the analyzer so the
/// backend never re-derives it from type names.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum CShape {
    #[default]
    Void,
    /// Numbers, `bool`, `char`, `byte`, as the C scalar of the same width and signedness.
    Scalar(CScalar),
    /// `string` as `const char*` (UTF-16 under `@marshal("lpwstr")`); `optional` maps `None` to `NULL`.
    Str { optional: bool },
    /// `CPtr` as `void*`; `optional` is `Option<CPtr>`.
    Ptr { optional: bool },
    /// An @unmanaged struct by value, as the C struct its layout describes.
    Struct,
    /// An `OwnedCPtr` result: the C pointer, freed by the C function `free` when Dream drops it.
    OwnedPtr { free: String },
    /// A `fun(...)` as a plain C function pointer.
    Func {
        params: Vec<CShape>,
        ret: Box<CShape>,
        optional: bool,
    },
    /// `NativeCallback<fun(...)>` as `(fn, void* user_data)`.
    Callback {
        params: Vec<CShape>,
        ret: Box<CShape>,
        optional: bool,
        user_data_last: bool,
    },
    /// `T[]` of unmanaged `T` as `T*` to its first element (`NULL` when empty), valid for the call.
    Array,
    /// A `ref` out-param: the address of the caller's storage.
    Ref,
}

impl CShape {
    /// True for a plain `fun` whose C signature differs from the Dream one, so C must call a
    /// per-target wrapper instead of the Dream function.
    pub fn needs_wrapper(&self) -> bool {
        match self {
            CShape::Func { params, ret, .. } => {
                !(ret.is_abi_identity() && params.iter().all(CShape::is_abi_identity))
            }
            _ => false,
        }
    }

    /// True when a callback of this shape can be the Dream function itself (identical C ABI).
    pub fn is_abi_identity(&self) -> bool {
        match self {
            CShape::Void => true,
            CShape::Scalar(s) => !s.is_narrow(),
            CShape::Func {
                params,
                ret,
                optional,
            } => !optional && ret.is_abi_identity() && params.iter().all(CShape::is_abi_identity),
            _ => false,
        }
    }
}

/// One monomorphized instance of a generic def, keyed by `(DefId, args)` — never a mangled string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonoInstance {
    pub def: DefId,
    pub args: Vec<TypeId>,
}

#[derive(Debug)]
pub struct HGlobal {
    pub id: GlobalId,
    pub name: String,
    pub ty: TypeId,
    pub is_const: bool,
    pub init: Option<HExpr>,
}

#[derive(Debug, Clone)]
pub struct HFunction {
    pub def: DefId,
    /// Resolved emitted symbol; no backend stage derives identity from numeric type handles.
    pub symbol: String,
    /// The semantic lookup name, retained for diagnostics and debug information.
    pub name: String,
    /// The instance args when this is a monomorphized body, empty otherwise.
    pub instance: Vec<TypeId>,
    pub params: Vec<HParam>,
    pub ret: TypeId,
    pub locals: Vec<HLocal>,
    pub body: Vec<HStmt>,
    pub is_async: bool,
    /// Path of the source file this function was declared in, naming it in debug info and panic
    /// locations. `None` for synthesized functions (module init, tests) with no source file.
    pub file: Option<String>,
    pub inline: InlineHint,
}

/// Inlining request from `@inline` / `@noinline` on the source declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InlineHint {
    #[default]
    Default,
    /// `@inline`: raised MIR inliner size budget, `alwaysinline` in LLVM.
    Prefer,
    /// `@noinline`: never inlined by MIR or LLVM.
    Never,
}

#[derive(Debug, Clone)]
pub struct HParam {
    pub local: LocalId,
    pub name: String,
    pub ty: TypeId,
    /// True for a `ref` parameter backed by a value-struct box (see
    /// `Analyzer::ref_box_type`/`docs/compiler/03-hir.md`): its MIR local must alias the caller's
    /// storage in place rather than take a private copy (`FunctionBuilder::new_ref_param`).
    pub is_ref: bool,
    /// True for a `take name: T` parameter: the callee takes ownership of the caller's +1.
    /// Unmarked / explicit `borrow` parameters leave this false (default borrow ABI).
    pub is_take: bool,
}

/// Declaration metadata for a function local (used by the backend to allocate slots and by RC
/// insertion to know which locals are references).
#[derive(Debug, Clone)]
pub struct HLocal {
    pub id: LocalId,
    pub name: String,
    pub ty: TypeId,
}
