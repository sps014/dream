//! `.dbg.json` writer for the wasmtime DAP adapter.

use dream_mir::{Mir, MirFunction};
use dream_types::{TyKind, TypeInterner};
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
    format!(
        "{{\"files\":[{}],\"functions\":[{}],\"types\":[{{\"kind\":\"scalar\",\"scalar\":\"int\"}}]}}",
        file_json, fns
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
            "{{\"name\":{},\"global\":{},\"type\":0}}",
            json_str(name),
            slot
        );
        slot += 1;
    }
    out.push(']');
    out
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
