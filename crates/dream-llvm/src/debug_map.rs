//! `.dbg.json` writer for the native DAP adapter.

use dream_mir::{Mir, MirFunction};
use dream_types::{PrimTy, TyKind, TypeInterner};
use std::fmt::Write as _;

pub fn debug_map_json(mir: &Mir, interner: &TypeInterner) -> String {
    let mut files: Vec<String> = Vec::new();
    for f in &mir.functions {
        if let Some(path) = &f.file {
            if !files.iter().any(|p| p == path) {
                files.push(path.clone());
            }
        }
    }
    let mut fns = String::new();
    for (i, f) in mir.functions.iter().enumerate() {
        if i > 0 {
            fns.push(',');
        }
        let _ = write!(
            fns,
            "{{\"id\":{},\"name\":{},\"vars\":{}}}",
            i,
            json_str(&f.name),
            fn_vars(f, interner)
        );
    }
    let mut file_json = String::new();
    for (i, f) in files.iter().enumerate() {
        if i > 0 {
            file_json.push(',');
        }
        file_json.push_str(&json_str(f));
    }
    let mut types = String::new();
    for (i, (_id, kind)) in interner.iter_kinds().enumerate() {
        if i > 0 {
            types.push(',');
        }
        types.push_str(&type_json(kind));
    }
    format!(
        "{{\"files\":[{}],\"functions\":[{}],\"types\":[{}]}}",
        file_json, fns, types
    )
}

fn fn_vars(func: &MirFunction, interner: &TypeInterner) -> String {
    let mut out = String::from("[");
    let mut slot = 0u32;
    let mut first = true;
    for decl in &func.locals {
        let Some(name) = decl.name.as_deref() else {
            continue;
        };
        if name.starts_with("__") {
            continue;
        }
        if matches!(interner.kind(decl.ty), TyKind::Void | TyKind::Error) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(
            out,
            "{{\"name\":{},\"global\":{},\"type\":{}}}",
            json_str(name),
            slot,
            decl.ty.0
        );
        slot += 1;
    }
    out.push(']');
    out
}

fn type_json(kind: &TyKind) -> String {
    match kind {
        TyKind::Prim(PrimTy::String) => "{\"kind\":\"string\"}".to_string(),
        TyKind::Prim(p) => format!("{{\"kind\":\"scalar\",\"scalar\":\"{}\"}}", p.name()),
        TyKind::Array(elem) => {
            format!("{{\"kind\":\"array\",\"elem\":{},\"stride\":4}}", elem.0)
        }
        _ => "{\"kind\":\"ref\"}".to_string(),
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
