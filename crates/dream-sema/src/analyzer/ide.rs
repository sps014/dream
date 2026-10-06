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
use crate::function_table::{FunctionIdentity, FunctionTableInfo};
use crate::union_table::UnionFieldInfo;
use dream_syntax::nodes::{Type, Visibility};
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;
use indexmap::{IndexMap as HashMap, IndexSet as HashSet};

mod identity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdeSource {
    pub file: Option<String>,
    pub start: usize,
    pub end: usize,
}

/// What a recorded source range resolved to. Names are source-level; keys are the analyzer's
/// member-lookup keys (mangled spellings like `List_int`, matching `struct_table`/`method_fn`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdeTarget {
    Resolved {
        def: dream_types::DefId,
        source: IdeSource,
        target: Box<IdeTarget>,
    },
    /// A function-local or parameter binding.
    Local { name: String },
    /// A top-level variable.
    Global { name: String },
    /// A resolved call target (free function, method, static method): the emitted function-table
    /// key plus the name as written at the call site.
    Callee {
        key: FunctionIdentity,
        label: String,
    },
    /// A `new T(...)` constructor call on the concrete (possibly monomorphized) type.
    Constructor {
        ty: dream_types::TypeId,
        display: String,
    },
    /// An `obj.field` access on the receiver type `owner`.
    Field {
        owner: dream_types::TypeId,
        name: String,
    },
    /// An `Enum.MEMBER` read on a C-style enum.
    EnumMember {
        owner: dream_types::TypeId,
        member: String,
    },
    /// A `Union.Variant` construction on the concrete union `owner`.
    UnionVariant {
        owner: dream_types::TypeId,
        variant: String,
    },
    /// A typed expression with no more specific target (tuple element, `.length`, index result).
    Expr,
}

/// The type of a recorded expression, rendered for editor consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeSummary {
    Named {
        ty: dream_types::TypeId,
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

    pub fn ty(&self) -> Option<dream_types::TypeId> {
        match self {
            TypeSummary::Named { ty, .. } => Some(*ty),
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
    pub primary_file: Option<String>,
    pub refs: Vec<IdeRef>,
    pub functions: HashMap<FunctionIdentity, FnSigOut>,
    pub methods: IndexMap<dream_types::TypeId, Vec<MemberInfo>>,
    pub future_types: HashSet<dream_types::TypeId>,
    /// Arrays and `string`, which carry the builtin `length` property.
    pub length_types: HashSet<dream_types::TypeId>,
    pub structs: IndexMap<dream_types::TypeId, Vec<FieldOut>>,
    pub enums: IndexMap<dream_types::TypeId, Vec<(String, i32)>>,
    pub unions: IndexMap<dream_types::TypeId, Vec<VariantOut>>,
    /// Source-level display name of every type keyed in the member tables.
    pub type_names: IndexMap<dream_types::TypeId, String>,
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
    /// Rendered detail (`count: int`, `push(value: T): int`, `Some(T)`).
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

    /// Every completable member of `ty`. Sorted by name; deterministic across runs.
    pub fn members_of(&self, key: dream_types::TypeId) -> Vec<MemberInfo> {
        let mut out: Vec<MemberInfo> = Vec::new();
        let mut seen = HashSet::new();

        if let Some(fields) = self.structs.get(&key) {
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

        if let Some(methods) = self.methods.get(&key) {
            for method in methods {
                if seen.insert(method.name.clone()) {
                    out.push(method.clone());
                }
            }
        }

        if let Some(variants) = self.enums.get(&key) {
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

        if let Some(variants) = self.unions.get(&key) {
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
        if self.length_types.contains(&key) && seen.insert("length".to_string()) {
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
}

impl<'a> Analyzer<'a> {
    /// Renders the accumulated IDE refs and signature tables into an owned snapshot. Requires
    /// `&mut self` only because rendering lowers AST types through the interner.
    pub fn ide_snapshot(&mut self) -> IdeSnapshot {
        let mut refs = std::mem::take(&mut self.ide_refs);
        let sources: Vec<(dream_types::DefId, IdeSource)> = self
            .ide_sources
            .iter()
            .map(|(def, source)| (*def, source.clone()))
            .collect();
        for (def, source) in sources {
            let name = self.type_ctx.defs.name(def).to_string();
            let target = match self.nominal_type_of(def) {
                None => IdeTarget::Callee {
                    key: (def, vec![]),
                    label: name,
                },
                Some(ty) => IdeTarget::Constructor { ty, display: name },
            };
            refs.push(IdeRef {
                start: source.start,
                end: source.end,
                file: source.file.clone(),
                target: IdeTarget::Resolved {
                    def,
                    source: source.clone(),
                    target: Box::new(target),
                },
                result: TypeSummary::Unknown,
            });
        }
        self.append_ide_member_declarations(&mut refs);
        refs.sort_by_key(|r| (r.start, r.end));
        refs.dedup_by(|a, b| {
            a.file == b.file && a.start == b.start && a.end == b.end && a.target == b.target
        });

        // Array-extend methods monomorphize lazily (on first use), so a receiver typed `int[]`
        // whose methods were never called would otherwise complete with nothing. Attach the
        // extension family for every array type the document actually handles; diagnostics go
        // to a throwaway bag because this runs purely for editor queries.
        let mut array_keys: Vec<dream_types::TypeId> = refs
            .iter()
            .filter_map(|r| match &r.result {
                TypeSummary::Named { ty, .. }
                    if matches!(
                        self.type_ctx.interner.kind(*ty),
                        dream_types::TyKind::Array(_)
                    ) =>
                {
                    Some(*ty)
                }
                _ => None,
            })
            .collect();
        array_keys.sort();
        array_keys.dedup();
        if !array_keys.is_empty() {
            let mut scratch = dream_diagnostics::DiagnosticBag::new(None);
            for key in &array_keys {
                self.ensure_array_collection(*key, &mut scratch);
            }
        }

        // Collect owned inputs first so rendering (`ty_display`, which needs `&mut self` to lower
        // types) never runs against a live borrow of the tables.
        let fn_inputs: Vec<(FunctionIdentity, FunctionTableInfo)> = self
            .function_table
            .functions
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let struct_inputs: Vec<StructFieldInput> = self
            .struct_table
            .structs
            .iter()
            .map(|(ty, info)| {
                (
                    *ty,
                    info.fields
                        .iter()
                        .map(|(fname, f)| (fname.clone(), f.ty, f.visibility))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let union_inputs: Vec<UnionVariantInput> = self
            .union_table
            .iter()
            .map(|(ty, info)| {
                (
                    *ty,
                    info.variants
                        .iter()
                        .map(|v| (v.name.clone(), v.discriminant, v.fields.clone()))
                        .collect(),
                )
            })
            .collect();
        let global_inputs: Vec<(String, dream_types::TypeId)> = self
            .globals
            .iter()
            .map(|g| (g.name.clone(), g.ty))
            .collect();

        let mut functions = HashMap::with_capacity(fn_inputs.len());
        for (key, info) in fn_inputs {
            functions.insert(key, self.render_fn_sig(&info));
        }
        let mut type_names = IndexMap::new();
        let mut methods = IndexMap::new();
        for ((receiver, member), identities) in &self.function_table.methods {
            type_names
                .entry(*receiver)
                .or_insert_with(|| self.type_id_display(*receiver));
            let entries = methods.entry(*receiver).or_insert_with(Vec::new);
            for identity in identities {
                if let Some(sig) = functions.get(identity)
                    && let Some(method) = render_method(member, sig) {
                        entries.push(method);
                    }
            }
        }

        let mut structs = IndexMap::with_capacity(struct_inputs.len());
        for (owner, fields) in struct_inputs {
            type_names
                .entry(owner)
                .or_insert_with(|| self.type_id_display(owner));
            let mut out: Vec<FieldOut> = fields
                .into_iter()
                .map(|(fname, ty, visibility)| FieldOut {
                    display: dream_types::display_name(
                        &self.type_ctx.interner,
                        &self.type_ctx.defs,
                        ty,
                    ),
                    name: fname,
                    public: visibility == Visibility::Public,
                })
                .collect();
            out.sort_by(|a, b| a.name.cmp(&b.name));
            structs.insert(owner, out);
        }

        let mut unions = IndexMap::with_capacity(union_inputs.len());
        for (owner, variants) in union_inputs {
            type_names
                .entry(owner)
                .or_insert_with(|| self.type_id_display(owner));
            let rendered = variants
                .into_iter()
                .map(|(vname, discriminant, fields)| VariantOut {
                    fields: fields
                        .into_iter()
                        .map(|f| VariantFieldOut {
                            display: dream_types::display_name(
                                &self.type_ctx.interner,
                                &self.type_ctx.defs,
                                f.ty,
                            ),
                            name: f.name,
                        })
                        .collect(),
                    name: vname,
                    discriminant,
                })
                .collect();
            unions.insert(owner, rendered);
        }

        let globals = global_inputs
            .into_iter()
            .map(|(name, ty)| GlobalOut {
                display: dream_types::display_name(
                    &self.type_ctx.interner,
                    &self.type_ctx.defs,
                    ty,
                ),
                name,
            })
            .collect();

        let enum_inputs: Vec<(dream_types::DefId, Vec<(String, i32)>)> = self
            .enum_table
            .iter()
            .map(|(def, members)| (*def, members.iter().map(|(n, v)| (n.clone(), *v)).collect()))
            .collect();
        let mut enums = IndexMap::with_capacity(enum_inputs.len());
        for (def, members) in enum_inputs {
            let owner = self.type_ctx.interner.enum_ty(def);
            type_names.insert(owner, self.type_ctx.defs.name(def).to_string());
            enums.insert(owner, members);
        }

        let future_types = self
            .type_ctx
            .interner
            .iter_kinds()
            .filter_map(|(ty, kind)| match kind {
                dream_types::TyKind::Struct(def, _)
                    if self.type_ctx.defs.name(*def) == dream_syntax::nodes::types::FUTURE_TYPE =>
                {
                    Some(ty)
                }
                _ => None,
            })
            .collect();
        let length_types = self
            .type_ctx
            .interner
            .iter_kinds()
            .filter_map(|(ty, kind)| {
                matches!(
                    kind,
                    dream_types::TyKind::Array(_)
                        | dream_types::TyKind::Prim(dream_types::PrimTy::String)
                )
                .then_some(ty)
            })
            .collect();
        IdeSnapshot {
            primary_file: None,
            refs,
            functions,
            methods,
            future_types,
            length_types,
            structs,
            enums,
            unions,
            type_names,
            globals,
        }
    }

    fn render_fn_sig(&mut self, info: &FunctionTableInfo) -> FnSigOut {
        let scope = self.type_ctx.scope();
        self.type_ctx.set_scope(info.identity.0.module);
        let is_method = info.param_names.first().is_some_and(|n| n == "this");
        let mut params = Vec::with_capacity(info.parameters.len());
        for (i, p) in info
            .parameters
            .iter()
            .enumerate()
            .skip(if is_method { 1 } else { 0 })
        {
            let name = info
                .param_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("arg{i}"));
            params.push(ParamOut {
                name,
                display: dream_types::display_name(
                    &self.type_ctx.interner,
                    &self.type_ctx.defs,
                    *p,
                ),
            });
        }
        let signature = FnSigOut {
            label: render_label(&info.name),
            params,
            ret: self.ty_display(&Self::async_return_type(
                info.is_async,
                info.return_type.clone(),
            )),
            is_static: info.is_static && !is_method,
            is_async: info.is_async,
        };
        self.type_ctx.set_scope(scope);
        signature
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
        let target = match (&target, &result) {
            (IdeTarget::Constructor { .. }, TypeSummary::Named { ty, .. }) => {
                self.resolve_ide_owner_target(*ty, None, target)
            }
            (IdeTarget::UnionVariant { variant, .. }, TypeSummary::Named { ty, .. }) => {
                let member = variant.clone();
                self.resolve_ide_owner_target(*ty, Some(&member), target)
            }
            _ => self.resolve_ide_target(target),
        };
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
        let id = self.type_ctx.lower(ty);
        self.ide_summary_id(id)
    }

    pub(in crate::analyzer) fn ide_summary_id(&self, id: dream_types::TypeId) -> TypeSummary {
        match self.type_ctx.interner.kind(id) {
            dream_types::TyKind::Error => return TypeSummary::Unknown,
            dream_types::TyKind::Tuple(elems) => {
                return TypeSummary::Tuple {
                    elems: elems
                        .iter()
                        .map(|elem| self.ide_summary_id(*elem))
                        .collect(),
                }
            }
            _ => {}
        }
        let display = dream_types::display_name(&self.type_ctx.interner, &self.type_ctx.defs, id);
        TypeSummary::Named { ty: id, display }
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

type StructFieldInput = (
    dream_types::TypeId,
    Vec<(String, dream_types::TypeId, Visibility)>,
);
type UnionVariantInput = (dream_types::TypeId, Vec<(String, i32, Vec<UnionFieldInfo>)>);

fn render_method(name: &str, sig: &FnSigOut) -> Option<MemberInfo> {
    if name == dream_syntax::nodes::types::CONSTRUCTOR_NAME || name.starts_with("set$") {
        return None;
    }
    if let Some(property) = name.strip_prefix("get$") {
        return Some(MemberInfo {
            kind: MemberKind::Property,
            name: property.to_string(),
            detail: format!("{property}: {}", sig.ret),
            is_static: sig.is_static,
        });
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
    Some(MemberInfo {
        kind: MemberKind::Method,
        name: name.to_string(),
        detail: format!("{name}({params}){ret}"),
        is_static: sig.is_static,
    })
}
