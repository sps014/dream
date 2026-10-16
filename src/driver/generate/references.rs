use super::model::{Attr, BodyNode, Decl, Function, Snapshot, TypeRef};
use indexmap::IndexSet;
use std::collections::VecDeque;

pub(super) enum Reference {
    Decl(Box<Decl>),
    Function(Box<Function>),
}

fn type_refs(ty: &TypeRef, out: &mut Vec<String>) {
    if !ty.decl.is_empty() {
        out.push(ty.decl.clone());
    }
    for arg in &ty.args {
        type_refs(arg, out);
    }
}

fn node_refs(node: &BodyNode, out: &mut Vec<String>) {
    if !node.symbol.is_empty()
        && !matches!(node.symbol_kind.as_str(), "local" | "parameter" | "global")
    {
        out.push(if node.overload.is_empty() {
            node.symbol.clone()
        } else {
            node.overload.clone()
        });
    }
    for ty in &node.types {
        type_refs(ty, out);
    }
    type_refs(&node.ty, out);
    for child in &node.children {
        node_refs(child, out);
    }
}

fn attribute_refs(attributes: &[Attr], out: &mut Vec<String>) {
    for arg in attributes.iter().flat_map(|a| &a.args) {
        if !arg.symbol.is_empty() {
            out.push(arg.symbol.clone());
        }
    }
}

fn function_refs(f: &Function, out: &mut Vec<String>) {
    attribute_refs(&f.attributes, out);
    for p in &f.params {
        attribute_refs(&p.attributes, out);
        type_refs(&p.ty, out);
    }
    type_refs(&f.ret, out);
    if let Some(body) = &f.body {
        for node in &body.statements {
            node_refs(node, out);
        }
    }
}

fn decl_refs(d: &Decl, out: &mut Vec<String>) {
    attribute_refs(&d.attributes, out);
    for ty in &d.implements {
        type_refs(ty, out);
    }
    for field in &d.fields {
        attribute_refs(&field.attributes, out);
        type_refs(&field.ty, out);
    }
    for variant in &d.variants {
        for field in &variant.fields {
            attribute_refs(&field.attributes, out);
            type_refs(&field.ty, out);
        }
    }
    for f in &d.methods {
        function_refs(f, out);
    }
}

/// Breadth-first source order keeps cycles finite and makes closure limits independent of DFS order.
pub(super) fn expand(
    snapshot: &mut Snapshot,
    mut resolve: impl FnMut(&str) -> Option<Reference>,
) -> Result<(), String> {
    let mut initial = Vec::new();
    let mut seen: IndexSet<String> = snapshot
        .decls
        .iter()
        .map(|d| d.id.clone())
        .chain(snapshot.functions.iter().map(|f| {
            if f.overload.is_empty() {
                f.id.clone()
            } else {
                f.overload.clone()
            }
        }))
        .collect();
    for d in &snapshot.decls {
        decl_refs(d, &mut initial);
    }
    for f in &snapshot.functions {
        function_refs(f, &mut initial);
    }
    for call in &snapshot.calls {
        initial.push(call.callee.clone());
        for ty in call.arg_types.iter().chain(&call.type_args) {
            type_refs(ty, &mut initial);
        }
    }
    let mut queue: VecDeque<_> = initial.into_iter().map(|id| (id, 0)).collect();
    while let Some((id, depth)) = queue.pop_front() {
        if seen.contains(&id) {
            continue;
        }
        let Some(reference) = resolve(&id) else {
            continue;
        };
        let mut next = Vec::new();
        let canonical = match &reference {
            Reference::Decl(d) => &d.id,
            Reference::Function(f) => {
                if f.overload.is_empty() {
                    &f.id
                } else {
                    &f.overload
                }
            }
        };
        if !seen.insert(canonical.clone()) {
            continue;
        }
        if seen.len() > 512 || depth > 32 {
            return Err(
                "generator referenced-declaration closure exceeds 512 declarations or 32 levels"
                    .to_string(),
            );
        }
        match reference {
            Reference::Decl(d) => {
                decl_refs(&d, &mut next);
                snapshot.referenced.push(*d);
            }
            Reference::Function(f) => {
                function_refs(&f, &mut next);
                snapshot.referenced_functions.push(*f);
            }
        }
        queue.extend(next.into_iter().map(|id| (id, depth + 1)));
    }
    Ok(())
}
