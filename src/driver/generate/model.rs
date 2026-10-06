//! The JSON contract between the compiler and a generator executable: the per-generator
//! [`Snapshot`] it reads (`--snapshot`) and the [`GenResult`] it writes (`--result`). Decoded on
//! the Dream side by `system.codegen` (`gen_decode.dream` / `GenContext.finish`). Paths are
//! project-relative and every collection is in source order, so the bytes are deterministic and
//! relocatable (the incremental cache keys on them).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Bump when the snapshot or result shape changes; `system.codegen` checks it.
pub const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Snapshot {
    pub version: u32,
    pub generator: String,
    pub generator_id: String,
    pub target: String,
    /// `dream.toml` `[[generators]].options`, decoded in Dream by `ctx.options<T>()`.
    pub options: BTreeMap<String, serde_json::Value>,
    pub additional_files: Vec<AdditionalFile>,
    /// Every type declaration in the program (user and stdlib), names and kinds only.
    pub index: Vec<IndexEntry>,
    /// Full detail for declarations carrying (or with members carrying) a trigger attribute.
    pub decls: Vec<Decl>,
    /// Top-level functions carrying a trigger attribute.
    pub functions: Vec<Function>,
    pub blocks: Vec<SyntaxSite>,
    pub calls: Vec<CallSite>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AdditionalFile {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Decl {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub module: String,
    pub visibility: String,
    pub location: Location,
    pub generics: Vec<String>,
    pub implements: Vec<TypeRef>,
    pub attributes: Vec<Attr>,
    pub fields: Vec<Field>,
    pub methods: Vec<Function>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Field {
    pub id: String,
    pub name: String,
    pub ty: TypeRef,
    pub visibility: String,
    pub attributes: Vec<Attr>,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub fields: Vec<Field>,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Function {
    pub id: String,
    pub name: String,
    pub is_static: bool,
    pub is_async: bool,
    pub visibility: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret: TypeRef,
    pub attributes: Vec<Attr>,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub attributes: Vec<Attr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Attr {
    /// `module::name` of the `@attribute` type, or `builtin::name` for compiler attributes.
    pub id: String,
    pub name: String,
    pub args: Vec<AttrArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AttrArg {
    /// `string` | `int` | `float` | `double` | `bool` | `enum`.
    pub kind: String,
    /// Unquoted string contents, literal text, or the dotted enum path.
    pub value: String,
}

/// A resolved type reference. `kind` is `prim` | `named` | `param` | `array` | `tuple` |
/// `function` | `void` | `unknown`; `decl` is the declaration identity for `named`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TypeRef {
    pub kind: String,
    pub name: String,
    pub display: String,
    pub mangled: String,
    pub decl: String,
    pub args: Vec<TypeRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SyntaxSite {
    pub id: String,
    pub name: String,
    pub body: String,
    pub splices: Vec<String>,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CallSite {
    pub id: String,
    /// Identity of the called declaration (one of the generator's `@on_call` paths).
    pub callee: String,
    pub type_args: Vec<TypeRef>,
    /// Statically known argument types (`unknown` when the syntactic pass cannot tell).
    pub arg_types: Vec<TypeRef>,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct GenResult {
    pub version: u32,
    pub generator: String,
    pub outputs: Vec<Output>,
    pub diagnostics: Vec<GenDiagnostic>,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Output {
    /// Rewrites a syntax site to a Dream expression.
    Replace { site: String, source: String },
    /// A full Dream source file at a generator-relative virtual path.
    File { path: String, source: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct GenDiagnostic {
    /// `error` | `warning`.
    pub severity: String,
    pub message: String,
    /// Snapshot identity the diagnostic is anchored to (decl, member, site or call), or empty.
    pub target: String,
}
