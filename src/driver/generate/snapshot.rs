//! Builds the per-generator [`Snapshot`]: a light index of every type, full detail only for
//! declarations the generator's `@on_attribute` triggers select, its own syntax sites, and its
//! `@on_call` call sites.

use super::call_sites::FoundCall;
use super::decls::{DeclIndex, IdSpans};
use super::model::*;
use super::paths::ProjectPaths;
use super::registry::{CallTrigger, RegisteredGenerator};
use super::sites::{Site, SiteKey};
use super::type_ref::{type_ref, unknown_type};
use crate::driver::source_loader::ProgramAccumulator;
use dream_abi::attributes::UserAttributes;
use dream_syntax::nodes::{AttributeArg, AttributeNode, FunctionNode, StructFieldNode, Visibility};
use dream_text::text_span::TextSpan;
use indexmap::IndexMap;

/// Program-wide facts shared by every generator's snapshot.
pub struct ProgramFacts<'f> {
    pub index: &'f DeclIndex,
    pub attributes: &'f UserAttributes,
    pub sites: &'f [Site],
    pub call_triggers: &'f [CallTrigger],
    pub calls: &'f [FoundCall],
    pub paths: &'f ProjectPaths,
    pub target: &'f str,
}

struct Builder<'f, 'p> {
    facts: &'p ProgramFacts<'f>,
    spans: IdSpans,
}

/// A generator's snapshot plus what maps its result back onto the program.
pub struct BuiltSnapshot {
    pub snapshot: Snapshot,
    pub spans: IdSpans,
    /// Syntax-site identity → rewrite key.
    pub sites: IndexMap<String, SiteKey>,
}

fn visibility(v: Visibility) -> String {
    match v {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Private => "private",
    }
    .to_string()
}

fn generic_names(
    params: &Option<Vec<dream_syntax::token::syntax_token::SyntaxToken>>,
) -> Vec<String> {
    params.iter().flatten().map(|p| p.text.clone()).collect()
}

impl Builder<'_, '_> {
    fn location(&self, file: Option<&str>, span: TextSpan) -> Location {
        Location {
            file: self.facts.paths.rel(file),
            line: span.line_no as u32,
            column: span.col_no as u32,
        }
    }

    fn mark(&mut self, id: &str, file: Option<&str>, span: TextSpan) {
        self.spans.insert(id, file, span);
    }

    fn attr(&self, a: &AttributeNode) -> Attr {
        let id = match self.facts.attributes.get(&a.name.text) {
            Some(decl) => decl.id.clone(),
            None => format!("builtin::{}", a.name.text),
        };
        Attr {
            id,
            name: a.name.text.clone(),
            args: a
                .args
                .iter()
                .map(|arg| AttrArg {
                    kind: match arg {
                        AttributeArg::String(_) => "string",
                        AttributeArg::Int(_) => "int",
                        AttributeArg::Float(_) => "float",
                        AttributeArg::Double(_) => "double",
                        AttributeArg::Bool(_) => "bool",
                        AttributeArg::Enum(_) => "enum",
                    }
                    .to_string(),
                    value: arg.semantic_value(),
                })
                .collect(),
        }
    }

    fn attrs(&self, attrs: &[AttributeNode]) -> Vec<Attr> {
        attrs.iter().map(|a| self.attr(a)).collect()
    }

    fn field(
        &mut self,
        owner: &str,
        f: &StructFieldNode,
        file: Option<&str>,
        generics: &[String],
    ) -> Field {
        let id = format!("{owner}.{}", f.name.text);
        self.mark(&id, file, f.name.position);
        Field {
            location: self.location(file, f.name.position),
            id,
            name: f.name.text.clone(),
            ty: type_ref(&f.field_type, self.facts.index, generics),
            visibility: visibility(f.visibility),
            attributes: self.attrs(&f.attributes),
        }
    }

    fn function(
        &mut self,
        owner: Option<&str>,
        module: &str,
        f: &FunctionNode<'_>,
        file: Option<&str>,
        outer_generics: &[String],
    ) -> Function {
        let id = match owner {
            Some(o) => format!("{o}.{}", f.name.text),
            None => dream_abi::attributes::decl_identity(module, &f.name.text),
        };
        self.mark(&id, file, f.name.position);
        let generics = generic_names(&f.generic_parameters);
        let mut scope: Vec<String> = outer_generics.to_vec();
        scope.extend(generics.iter().cloned());
        Function {
            location: self.location(file, f.name.position),
            id,
            name: f.name.text.clone(),
            is_static: f.is_static,
            is_async: f.is_async,
            visibility: visibility(f.visibility),
            generics,
            params: f
                .parameters
                .iter()
                .filter(|p| p.name.text != "this")
                .map(|p| Param {
                    name: p.name.text.clone(),
                    ty: type_ref(&p.type_, self.facts.index, &scope),
                    attributes: self.attrs(&p.attributes),
                })
                .collect(),
            ret: match &f.return_type {
                Some(t) => type_ref(t, self.facts.index, &scope),
                None => type_ref(&dream_syntax::nodes::Type::Void, self.facts.index, &scope),
            },
            attributes: self.attrs(&f.attributes),
        }
    }
}

fn carries(attrs: &[AttributeNode], ids: &[String], registry: &UserAttributes) -> bool {
    attrs.iter().any(|a| {
        registry
            .get(&a.name.text)
            .is_some_and(|decl| ids.contains(&decl.id))
    })
}

fn function_carries(f: &FunctionNode<'_>, ids: &[String], registry: &UserAttributes) -> bool {
    carries(&f.attributes, ids, registry)
        || f.parameters
            .iter()
            .any(|p| carries(&p.attributes, ids, registry))
}

/// True when any declaration, member or top-level function carries one of `ids`.
pub fn program_carries(
    acc: &ProgramAccumulator<'_>,
    ids: &[String],
    registry: &UserAttributes,
) -> bool {
    if ids.is_empty() {
        return false;
    }
    let fields = |fs: &[StructFieldNode]| fs.iter().any(|f| carries(&f.attributes, ids, registry));
    acc.all_structs.iter().any(|s| {
        carries(&s.attributes, ids, registry)
            || fields(&s.fields)
            || s.methods.iter().any(|m| function_carries(m, ids, registry))
    }) || acc.all_enums.iter().any(|e| {
        carries(&e.attributes, ids, registry)
            || e.variants.iter().any(|v| fields(&v.fields))
            || e.methods.iter().any(|m| function_carries(m, ids, registry))
    }) || acc.all_interfaces.iter().any(|i| {
        carries(&i.attributes, ids, registry)
            || i.methods.iter().any(|m| function_carries(m, ids, registry))
    }) || acc
        .all_functions
        .iter()
        .any(|f| function_carries(f, ids, registry))
}

fn read_additional_files(
    registered: &RegisteredGenerator,
    paths: &ProjectPaths,
) -> Result<Vec<AdditionalFile>, String> {
    let Some(entry) = &registered.entry else {
        return Ok(Vec::new());
    };
    entry
        .additional_files
        .iter()
        .map(|rel| {
            let path = paths.root.join(rel);
            std::fs::read_to_string(&path)
                .map(|contents| AdditionalFile {
                    path: rel.replace('\\', "/"),
                    contents,
                })
                .map_err(|e| {
                    format!(
                        "generator '{}': cannot read additional file '{}': {e}",
                        registered.name,
                        path.display()
                    )
                })
        })
        .collect()
}

pub fn build_snapshot(
    acc: &ProgramAccumulator<'_>,
    facts: &ProgramFacts<'_>,
    registered: &RegisteredGenerator,
) -> Result<BuiltSnapshot, String> {
    let mut b = Builder {
        facts,
        spans: IdSpans::default(),
    };
    let registry = facts.attributes;
    let ids = &registered.attribute_triggers;
    let mut snapshot = Snapshot {
        version: SNAPSHOT_VERSION,
        generator: registered.name.clone(),
        generator_id: registered.id.clone(),
        target: facts.target.to_string(),
        options: registered
            .entry
            .as_ref()
            .map(|e| e.options.clone())
            .unwrap_or_default(),
        additional_files: read_additional_files(registered, facts.paths)?,
        index: facts
            .index
            .types()
            .map(|e| IndexEntry {
                id: e.id.clone(),
                name: e.name.clone(),
                kind: e.kind.as_str().to_string(),
            })
            .collect(),
        ..Snapshot::default()
    };
    if !ids.is_empty() {
        for s in &acc.all_structs {
            let selected = carries(&s.attributes, ids, registry)
                || s.fields
                    .iter()
                    .any(|f| carries(&f.attributes, ids, registry))
                || s.methods.iter().any(|m| function_carries(m, ids, registry));
            if !selected {
                continue;
            }
            let file = s.file_path.as_deref();
            let module = super::decls::module_of(acc, file);
            let id = dream_abi::attributes::decl_identity(&module, &s.name.text);
            b.mark(&id, file, s.name.position);
            let generics = generic_names(&s.generic_parameters);
            let fields = s
                .fields
                .iter()
                .map(|f| b.field(&id, f, file, &generics))
                .collect();
            let methods = s
                .methods
                .iter()
                .map(|m| b.function(Some(&id), &module, m, file, &generics))
                .collect();
            snapshot.decls.push(Decl {
                location: b.location(file, s.name.position),
                name: s.name.text.clone(),
                kind: if s.is_value { "struct" } else { "class" }.to_string(),
                module,
                visibility: visibility(s.visibility),
                implements: s
                    .implements
                    .iter()
                    .map(|t| type_ref(t, facts.index, &generics))
                    .collect(),
                attributes: b.attrs(&s.attributes),
                generics,
                fields,
                methods,
                variants: Vec::new(),
                id,
            });
        }
        for e in &acc.all_enums {
            let selected = carries(&e.attributes, ids, registry)
                || e.variants.iter().any(|v| {
                    v.fields
                        .iter()
                        .any(|f| carries(&f.attributes, ids, registry))
                })
                || e.methods.iter().any(|m| function_carries(m, ids, registry));
            if !selected {
                continue;
            }
            let file = e.file_path.as_deref();
            let module = super::decls::module_of(acc, file);
            let id = dream_abi::attributes::decl_identity(&module, &e.name.text);
            b.mark(&id, file, e.name.position);
            let generics = generic_names(&e.generic_parameters);
            let variants = e
                .variants
                .iter()
                .map(|v| {
                    let vid = format!("{id}.{}", v.name.text);
                    b.mark(&vid, file, v.name.position);
                    Variant {
                        location: b.location(file, v.name.position),
                        fields: v
                            .fields
                            .iter()
                            .map(|f| b.field(&vid, f, file, &generics))
                            .collect(),
                        name: v.name.text.clone(),
                        id: vid,
                    }
                })
                .collect();
            let methods = e
                .methods
                .iter()
                .map(|m| b.function(Some(&id), &module, m, file, &generics))
                .collect();
            snapshot.decls.push(Decl {
                location: b.location(file, e.name.position),
                name: e.name.text.clone(),
                kind: if e.is_data_enum() { "union" } else { "enum" }.to_string(),
                module,
                visibility: "public".to_string(),
                implements: Vec::new(),
                attributes: b.attrs(&e.attributes),
                generics,
                fields: Vec::new(),
                methods,
                variants,
                id,
            });
        }
        for i in &acc.all_interfaces {
            let selected = carries(&i.attributes, ids, registry)
                || i.methods.iter().any(|m| function_carries(m, ids, registry));
            if !selected {
                continue;
            }
            let file = i.file_path.as_deref();
            let module = super::decls::module_of(acc, file);
            let id = dream_abi::attributes::decl_identity(&module, &i.name.text);
            b.mark(&id, file, i.name.position);
            let generics = generic_names(&i.generic_parameters);
            let methods = i
                .methods
                .iter()
                .map(|m| b.function(Some(&id), &module, m, file, &generics))
                .collect();
            snapshot.decls.push(Decl {
                location: b.location(file, i.name.position),
                name: i.name.text.clone(),
                kind: "interface".to_string(),
                module,
                visibility: "public".to_string(),
                implements: Vec::new(),
                attributes: b.attrs(&i.attributes),
                generics,
                fields: Vec::new(),
                methods,
                variants: Vec::new(),
                id,
            });
        }
        for f in &acc.all_functions {
            if function_carries(f, ids, registry) {
                let file = f.file_path.as_deref();
                let module = super::decls::module_of(acc, file);
                let func = b.function(None, &module, f, file, &[]);
                snapshot.functions.push(func);
            }
        }
    }
    let mut site_ids = IndexMap::new();
    if registered.syntax_block {
        for site in facts.sites.iter().filter(|s| s.name == registered.name) {
            let file = Some(site.key.0.as_str()).filter(|f| !f.is_empty());
            let location = b.location(file, site.name_span);
            let id = format!(
                "site:{}:{}:{}",
                location.file, location.line, location.column
            );
            b.mark(&id, file, site.name_span);
            site_ids.insert(id.clone(), site.key.clone());
            snapshot.blocks.push(SyntaxSite {
                id,
                name: site.name.clone(),
                body: site.body.clone(),
                splices: site.splices.clone(),
                location,
            });
        }
    }
    for call in facts.calls {
        let trigger = &facts.call_triggers[call.trigger];
        if !registered.call_triggers.iter().any(|t| t.id == trigger.id) {
            continue;
        }
        let file = call.file.as_deref();
        let location = b.location(file, call.token.position);
        let id = format!(
            "call:{}:{}:{}",
            location.file, location.line, location.column
        );
        b.mark(&id, file, call.token.position);
        snapshot.calls.push(CallSite {
            id,
            callee: trigger.id.clone(),
            type_args: call
                .type_args
                .iter()
                .map(|t| type_ref(t, facts.index, &[]))
                .collect(),
            arg_types: call
                .arg_types
                .iter()
                .map(|t| match t {
                    Some(t) => type_ref(t, facts.index, &[]),
                    None => unknown_type(),
                })
                .collect(),
            location,
        });
    }
    Ok(BuiltSnapshot {
        snapshot,
        spans: b.spans,
        sites: site_ids,
    })
}

/// True when the snapshot gives the generator anything to do.
pub fn has_inputs(snapshot: &Snapshot) -> bool {
    !snapshot.decls.is_empty()
        || !snapshot.functions.is_empty()
        || !snapshot.blocks.is_empty()
        || !snapshot.calls.is_empty()
}
