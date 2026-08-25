//! IDE-facing query layer. During body analysis the resolver records every name/member/call it
//! resolves — `(source span) -> what it resolved to + the expression's type` — into a side table.
//! After analysis (clean *or* failed; the table does not depend on HIR emission) [`Analyzer::ide_snapshot`]
//! renders it, together with the signature/layout tables, into an owned [`IdeSnapshot`] that an
//! editor client can hold without keeping the analyzer's arena alive.
//!
//! This is deliberately a one-way producer: recording never influences analysis results, so the
//! compiler pipeline is unaffected and determinism of emitted output is untouched (snapshot
//! queries sort their outputs before returning them).

//! (module lives under `analyzer` so it can read the resolver's internals; the public surface is
//! re-exported from `analyzer`.)

use super::Analyzer;
use crate::function_table::FunctionTableInfo;
use crate::union_table::UnionFieldInfo;
use dream_syntax::nodes::{Type, Visibility};
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;
use std::collections::HashMap;

/// What a recorded source range resolved to. Names are source-level; keys are the analyzer's
/// member-lookup keys (mangled spellings like `List_int`, matching `struct_table`/`method_fn`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdeTarget {
    /// A function-local or parameter binding.
    Local { name: String },
    /// A top-level variable.
    Global { name: String },
    /// A resolved call target (free function, method, static method): the emitted function-table
    /// key plus the name as written at the call site.
    Callee { key: String, label: String },
    /// A `new T(...)` constructor call; `type_key` is the concrete (possibly monomorphized) type.
    Constructor { type_key: String },
    /// An `obj.field` access; `type_key` is the receiver's member-lookup key.
    Field { type_key: String, name: String },
    /// An `Enum.MEMBER` read on a C-style enum.
    EnumMember { enum_name: String, member: String },
    /// A `Union.Variant` construction; `union_key` is the concrete union's lookup key.
    UnionVariant { union_key: String, variant: String },
    /// A typed expression with no more specific target (tuple element, `.length`, index result).
    Expr,
}

/// The type of a recorded expression, rendered for editor consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeSummary {
    Named {
        /// Member-lookup key (`int`, `List_int`, `Point[]`, ...) when the type is addressable in
        /// the signature tables; `None` for unknown/poison types.
        key: Option<String>,
        display: String,
    },
    Tuple {
        elems: Vec<TypeSummary>,
    },
    Unknown,
}

impl TypeSummary {
    pub fn display(&self) -> &str {
        match self {
            TypeSummary::Named { display, .. } => display,
            TypeSummary::Tuple { .. } => "(…)",
            TypeSummary::Unknown => "unknown",
        }
    }

    pub fn key(&self) -> Option<&str> {
        match self {
            TypeSummary::Named { key, .. } => key.as_deref(),
            _ => None,
        }
    }
}

/// One recorded resolution: the byte range of the written token(s), what they resolved to, and
/// the resulting expression type. `file` is the source file the range belongs to (`None` for
/// synthesized/unknown origins) — the analyzer processes the whole merged program (imports +
/// prelude), so offsets are only meaningful within their own file's text.
#[derive(Debug, Clone)]
pub struct IdeRef {
    pub start: usize,
    pub end: usize,
    pub file: Option<String>,
    pub target: IdeTarget,
    pub result: TypeSummary,
}

/// A rendered parameter of a function/method signature.
#[derive(Debug, Clone)]
pub struct ParamOut {
    pub name: String,
    pub display: String,
}

/// A rendered function/method signature, keyed by emitted function-table key.
#[derive(Debug, Clone)]
pub struct FnSigOut {
    /// Name as written in source (`push`, not `List_int_push`).
    pub label: String,
    /// Parameters excluding the implicit `this` receiver.
    pub params: Vec<ParamOut>,
    pub ret: String,
    pub is_static: bool,
    pub is_async: bool,
}

/// A rendered struct/class field.
#[derive(Debug, Clone)]
pub struct FieldOut {
    pub name: String,
    pub display: String,
    pub public: bool,
}

/// A rendered payload field of a discriminated-union variant.
#[derive(Debug, Clone)]
pub struct VariantFieldOut {
    pub name: String,
    pub display: String,
}

/// A rendered discriminated-union variant.
#[derive(Debug, Clone)]
pub struct VariantOut {
    pub name: String,
    pub discriminant: i32,
    pub fields: Vec<VariantFieldOut>,
}

/// A rendered top-level global.
#[derive(Debug, Clone)]
pub struct GlobalOut {
    pub name: String,
    pub display: String,
}

/// The owned, editor-ready extract of one analysis run. Cheap to hold between keystrokes (no
/// arena references); all queries return deterministic orderings.
#[derive(Debug, Clone, Default)]
pub struct IdeSnapshot {
    pub refs: Vec<IdeRef>,
    pub functions: HashMap<String, FnSigOut>,
    pub structs: HashMap<String, Vec<FieldOut>>,
    pub enums: IndexMap<String, Vec<(String, i32)>>,
    pub unions: IndexMap<String, Vec<VariantOut>>,
    pub globals: Vec<GlobalOut>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Field,
    Method,
    Property,
    EnumVariant,
    UnionVariant,
}

/// One completable/hoverable member of a type.
#[derive(Debug, Clone)]
pub struct MemberInfo {
    pub kind: MemberKind,
    pub name: String,
    /// Rendered detail (`count: int`, `push(value: T): int`, `Some(value: T)`).
    pub detail: String,
    pub is_static: bool,
}

impl IdeSnapshot {
    /// Exact-span ref lookup (the common case: a token's own span was recorded).
    pub fn ref_at(&self, start: usize, end: usize) -> Option<&IdeRef> {
        let idx = self
            .refs
            .binary_search_by(|r| {
                if r.start < start {
                    std::cmp::Ordering::Less
                } else if r.start > start {
                    std::cmp::Ordering::Greater
                } else if r.end < end {
                    std::cmp::Ordering::Less
                } else if r.end > end {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        Some(&self.refs[idx])
    }

    /// The ref whose range contains `offset`.
    pub fn ref_covering(&self, offset: usize) -> Option<&IdeRef> {
        let idx = self
            .refs
            .partition_point(|r| r.start <= offset)
            .checked_sub(1)?;
        let r = &self.refs[idx];
        (offset < r.end).then_some(r)
    }

    /// Every completable member of the type addressed by `key` (a member-lookup key such as
    /// `Point`, `List_int`, `string`, `int[]`). Sorted by name; deterministic across runs.
    pub fn members_of(&self, key: &str) -> Vec<MemberInfo> {
        let mut out: Vec<MemberInfo> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        if let Some(fields) = self.structs.get(key) {
            for f in fields {
                if seen.insert(f.name.clone()) {
                    out.push(MemberInfo {
                        kind: MemberKind::Field,
                        name: f.name.clone(),
                        detail: format!("{}: {}", f.name, f.display),
                        is_static: false,
                    });
                }
            }
        }

        let prefix = format!("{key}_");
        self.push_methods(prefix, &mut out, &mut seen);
        // Array-extend methods are registered per *element* type (`int__arr_get`), not per
        // array spelling — merge that family too for `T[]` receivers.
        if let Some(elem) = key.strip_suffix("[]") {
            self.push_methods(format!("{elem}__arr_"), &mut out, &mut seen);
            self.push_methods(format!("{elem}[]_"), &mut out, &mut seen);
        }

        if let Some(variants) = self.enums.get(key) {
            for (name, value) in variants {
                if seen.insert(name.clone()) {
                    out.push(MemberInfo {
                        kind: MemberKind::EnumVariant,
                        name: name.clone(),
                        detail: format!("{name} = {value}"),
                        is_static: true,
                    });
                }
            }
        }

        if let Some(variants) = self.unions.get(key) {
            for v in variants {
                let fields = v
                    .fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.display))
                    .collect::<Vec<_>>()
                    .join(", ");
                let detail = if fields.is_empty() {
                    v.name.clone()
                } else {
                    format!("{}({})", v.name, fields)
                };
                if seen.insert(v.name.clone()) {
                    out.push(MemberInfo {
                        kind: MemberKind::UnionVariant,
                        name: v.name.clone(),
                        detail,
                        is_static: true,
                    });
                }
            }
        }

        // `length` on arrays/strings is a builtin property (see `analyze_member_access`).
        if (key.ends_with("[]") || key == "string") && seen.insert("length".to_string()) {
            out.push(MemberInfo {
                kind: MemberKind::Property,
                name: "length".to_string(),
                detail: "length: int".to_string(),
                is_static: false,
            });
        }

        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Appends the methods of one registration family: every function whose emitted key starts
    /// with `prefix` (`{Type}_{method}`, `{elem}__arr_{method}`, …). Deduplicated through
    /// `seen`, which may already contain field/variant names.
    fn push_methods(
        &self,
        prefix: String,
        out: &mut Vec<MemberInfo>,
        seen: &mut std::collections::HashSet<String>,
    ) {
        for (emitted, sig) in &self.functions {
            let Some(rest) = emitted.strip_prefix(prefix.as_str()) else {
                continue;
            };
            // Overload-mangled keys append `.TypeId...`; getters/setters use internal `$` names.
            let base = rest.split('.').next().unwrap_or(rest);
            if base.is_empty() || base == "constructor" {
                continue;
            }
            if let Some(prop) = base.strip_prefix("get$") {
                if !prop.is_empty() && seen.insert(format!("get${prop}")) {
                    out.push(MemberInfo {
                        kind: MemberKind::Property,
                        name: prop.to_string(),
                        detail: format!("{}: {}", prop, sig.ret),
                        is_static: false,
                    });
                }
                continue;
            }
            if base.starts_with("set$") {
                continue;
            }
            let params = sig
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.display))
                .collect::<Vec<_>>()
                .join(", ");
            let ret = if sig.ret == "void" {
                String::new()
            } else {
                format!(": {}", sig.ret)
            };
            if seen.insert(base.to_string()) {
                out.push(MemberInfo {
                    kind: MemberKind::Method,
                    name: base.to_string(),
                    detail: format!("{}({}){}", base, params, ret),
                    is_static: sig.is_static,
                });
            }
        }
    }
}

impl<'a> Analyzer<'a> {
    /// Renders the accumulated IDE refs and signature tables into an owned snapshot. Requires
    /// `&mut self` only because rendering lowers AST types through the interner.
    pub fn ide_snapshot(&mut self) -> IdeSnapshot {
        let mut refs = std::mem::take(&mut self.ide_refs);
        refs.sort_by_key(|r| (r.start, r.end));

        // Array-extend methods monomorphize lazily (on first use), so a receiver typed `int[]`
        // whose methods were never called would otherwise complete with nothing. Attach the
        // extension family for every array type the document actually handles; diagnostics go
        // to a throwaway bag because this runs purely for editor queries.
        let mut array_keys: Vec<String> = refs
            .iter()
            .filter_map(|r| match &r.result {
                TypeSummary::Named { key: Some(k), .. } if k.ends_with("[]") => Some(k.clone()),
                _ => None,
            })
            .collect();
        array_keys.sort();
        array_keys.dedup();
        if !array_keys.is_empty() {
            let mut scratch = dream_diagnostics::DiagnosticBag::new(None);
            for key in &array_keys {
                self.ensure_array_collection(key, &mut scratch);
            }
        }

        // Collect owned inputs first so rendering (`ty_display`, which needs `&mut self` to lower
        // types) never runs against a live borrow of the tables.
        let fn_inputs: Vec<(String, FunctionTableInfo)> = self
            .function_table
            .functions
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let struct_inputs: Vec<StructFieldInput> = self
            .struct_table
            .structs
            .iter()
            .map(|(name, info)| {
                (
                    name.clone(),
                    info.fields
                        .iter()
                        .map(|(fname, f)| (fname.clone(), f.type_.clone(), f.visibility))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let union_inputs: Vec<UnionVariantInput> = self
            .union_table
            .iter()
            .map(|(name, info)| {
                (
                    name.clone(),
                    info.variants
                        .iter()
                        .map(|v| (v.name.clone(), v.discriminant, v.fields.clone()))
                        .collect(),
                )
            })
            .collect();
        let global_inputs: Vec<(String, String)> = self
            .globals
            .iter()
            .map(|g| (g.name.clone(), g.type_str.clone()))
            .collect();

        let mut functions = HashMap::with_capacity(fn_inputs.len());
        for (key, info) in fn_inputs {
            functions.insert(key, self.render_fn_sig(&info));
        }

        let mut structs = HashMap::with_capacity(struct_inputs.len());
        for (name, fields) in struct_inputs {
            let mut out: Vec<FieldOut> = fields
                .into_iter()
                .map(|(fname, ty, visibility)| FieldOut {
                    display: self.ty_display(&ty),
                    name: fname,
                    public: visibility == Visibility::Public,
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            structs.insert(name, out);
        }

        let mut unions = IndexMap::with_capacity(union_inputs.len());
        for (name, variants) in union_inputs {
            let rendered = variants
                .into_iter()
                .map(|(vname, discriminant, fields)| VariantOut {
                    fields: fields
                        .into_iter()
                        .map(|f| VariantFieldOut {
                            display: self.ty_display(&f.type_),
                            name: f.name,
                        })
                        .collect(),
                    name: vname,
                    discriminant,
                })
                .collect();
            unions.insert(name, rendered);
        }

        let globals = global_inputs
            .into_iter()
            .map(|(name, type_str)| GlobalOut {
                display: self.ty_display(&Self::concrete_type_from_str(&type_str)),
                name,
            })
            .collect();

        let enums: IndexMap<String, Vec<(String, i32)>> = self
            .enum_table
            .iter()
            .map(|(name, members)| {
                (
                    name.clone(),
                    members.iter().map(|(n, v)| (n.clone(), *v)).collect(),
                )
            })
            .collect();

        IdeSnapshot {
            refs,
            functions,
            structs,
            enums,
            unions,
            globals,
        }
    }

    fn render_fn_sig(&mut self, info: &FunctionTableInfo) -> FnSigOut {
        let types = if info.parameter_types.len() == info.parameters.len() {
            info.parameter_types.clone()
        } else {
            info.parameters.iter().map(|p| Self::type_from_name(p)).collect()
        };
        let is_method = info.param_names.first().is_some_and(|n| n == "this");
        let mut params = Vec::with_capacity(types.len());
        for (i, p) in types.iter().enumerate().skip(if is_method { 1 } else { 0 }) {
            let name = info
                .param_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("arg{i}"));
            params.push(ParamOut {
                name,
                display: self.ty_display(p),
            });
        }
        FnSigOut {
            label: render_label(&info.name),
            params,
            ret: self.ty_display(&Self::async_return_type(
                info.is_async,
                info.return_type.clone(),
            )),
            is_static: info.is_static && !is_method,
            is_async: info.is_async,
        }
    }

    /// Records one resolution. Synthesized spans (`start == end`) are skipped so desugar-time
    /// clones never shadow real user-source ranges. The ref is tagged with the file currently
    /// being analyzed so consumers never map a span onto the wrong document.
    pub(in crate::analyzer) fn record_ide_ref(
        &mut self,
        span: TextSpan,
        target: IdeTarget,
        result: TypeSummary,
    ) {
        if span.end <= span.start {
            return;
        }
        self.ide_refs.push(IdeRef {
            start: span.start,
            end: span.end,
            file: self.current_file.as_ref().map(|f| f.to_string()),
            target,
            result,
        });
    }

    /// Summarizes an AST type for the IDE table (lookup key + pretty display).
    pub(in crate::analyzer) fn ide_summary(&mut self, ty: &Type) -> TypeSummary {
        if ty.is_unknown() {
            return TypeSummary::Unknown;
        }
        if let Type::Tuple(elems) = ty {
            return TypeSummary::Tuple {
                elems: elems.iter().map(|e| self.ide_summary(e)).collect(),
            };
        }
        TypeSummary::Named {
            key: Some(ty.get_type()),
            display: self.ty_display(ty),
        }
    }

    /// Like [`Self::ide_summary`] for a tuple's element list.
    pub(in crate::analyzer) fn ide_tuple_elems(&mut self, ty: &Type) -> Vec<TypeSummary> {
        match ty {
            Type::Tuple(elems) => elems.iter().map(|e| self.ide_summary(e)).collect(),
            other => vec![self.ide_summary(other)],
        }
    }
}

/// Source-level label for a non-method function-table entry: strips overload-suffixes and
/// module qualification but keeps generic monomorphization visible (`sort_int_string` stays
/// readable as-is rather than guessing at the template name).
fn render_label(emitted: &str) -> String {
    let no_module = emitted.rsplit("::").next().unwrap_or(emitted);
    no_module.split('.').next().unwrap_or(no_module).to_string()
}

type StructFieldInput = (String, Vec<(String, Type, Visibility)>);
type UnionVariantInput = (String, Vec<(String, i32, Vec<UnionFieldInfo>)>);
