//! Debugger-readable local names for emitted C.
//!
//! MIR locals are positional (`l{n}` in the emitted C). Params and user `let`s carry source names
//! ([`crate::LocalDecl::name`]); [`apply_local_names`] rewrites a finished [`Func`] so those
//! locals appear under sanitized, collision-free source names instead. Everything is resolved up
//! front — against backend-synthetic namespaces (`l{n}`/`t{n}`/`g{n}`) and every non-local
//! identifier the body references — and applied in one sweep, so prototypes derived from the
//! rewritten params always match the rewritten body.
//!
//! Unnamed temporaries keep their positional `l{n}` spelling.

use std::collections::HashSet;

use super::ast::{CTy, Expr, Func, Stmt};
use crate::backend::shared::names::sanitize_ident;
use crate::MirFunction;

/// True when `s` belongs to a backend-synthetic identifier namespace (`l3` locals, `t12`
/// builder temps, `g7` globals): positional spellings a source name must never shadow.
fn is_synthetic(s: &str) -> bool {
    let numbered = |prefix: char| match s.strip_prefix(prefix) {
        Some(rest) => !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    };
    numbered('l') || numbered('t') || numbered('g')
}

/// Renames `func`'s params and body locals per `f`'s source names. No-op when no local has one.
///
/// Only params currently spelled positionally (`l{n}`) are touched — synthetic params like the
/// coroutine polls' `__self` are left alone.
pub(super) fn apply_local_names(f: &MirFunction, func: &mut Func) {
    let names = resolve_names(f, &func.body);
    let renamed = names
        .iter()
        .enumerate()
        .any(|(i, n)| *n != format!("l{i}"));
    if !renamed {
        return;
    }
    for p in func.params.iter_mut() {
        if let Some(idx) = parse_numbered(&p.name, 'l') {
            if let Some(n) = names.get(idx) {
                p.name = n.clone();
            }
        }
    }
    for stmt in &mut func.body {
        rewrite_stmt(stmt, &names);
    }
}

/// Final per-local C identifiers, index-aligned with `f.locals`.
fn resolve_names(f: &MirFunction, body: &[Stmt]) -> Vec<String> {
    let mut names = base_names(f);
    // A source-derived name that shadows any identifier the body references (runtime helpers,
    // other function symbols, macros, string-table objects) breaks those references; suffix it.
    let reserved = body_reserved(body);
    let mut finals: HashSet<String> = names.iter().cloned().collect();
    for name in names.iter_mut() {
        if is_synthetic(name) {
            continue;
        }
        let base = std::mem::take(name);
        finals.remove(&base);
        let mut cand = base.clone();
        let mut k = 2;
        while reserved.contains(&cand) || finals.contains(&cand) {
            cand = format!("{base}_{k}");
            k += 1;
        }
        *name = cand.clone();
        finals.insert(cand);
    }
    names
}

/// Sanitized candidate per local: source-derived and deduped among themselves; unnamed locals
/// fall back to their positional `l{i}` spelling.
fn base_names(f: &MirFunction) -> Vec<String> {
    let mut used: HashSet<String> = HashSet::new();
    f.locals
        .iter()
        .enumerate()
        .map(|(i, decl)| {
            let fallback = format!("l{i}");
            let Some(base) = decl.name.as_deref().and_then(sanitize_ident) else {
                return fallback;
            };
            if !is_synthetic(&base) && !used.contains(&base) {
                used.insert(base.clone());
                return base;
            }
            let mut k = 2;
            loop {
                let cand = format!("{base}_{k}");
                if !is_synthetic(&cand) && !used.contains(&cand) {
                    used.insert(cand.clone());
                    return cand;
                }
                k += 1;
            }
        })
        .collect()
}

/// Every identifier the body references that is not itself a positional local/temp/global:
/// call targets, runtime helpers, macros, string-table objects. Locals must not take any of
/// these names.
fn body_reserved(body: &[Stmt]) -> HashSet<String> {
    let mut set = HashSet::new();
    for s in body {
        collect_stmt(s, &mut set);
    }
    set.retain(|s| !is_synthetic(s));
    set
}

fn collect_stmt(stmt: &Stmt, set: &mut HashSet<String>) {
    match stmt {
        Stmt::Expr(e) => collect_expr(e, set),
        Stmt::Assign { dest, src } => {
            collect_expr(dest, set);
            collect_expr(src, set);
        }
        Stmt::Decl { ty, init, .. } => {
            collect_ty(ty, set);
            if let Some(e) = init {
                collect_expr(e, set);
            }
        }
        Stmt::If {
            cond,
            then_s,
            else_s,
        } => {
            collect_expr(cond, set);
            collect_stmt(then_s, set);
            if let Some(s) = else_s {
                collect_stmt(s, set);
            }
        }
        Stmt::Switch { expr, arms } => {
            collect_expr(expr, set);
            for arm in arms {
                for s in &arm.body {
                    collect_stmt(s, set);
                }
            }
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            collect_stmt(init, set);
            collect_expr(cond, set);
            collect_stmt(step, set);
            collect_stmt(body, set);
        }
        Stmt::GotoIndirect(e) => collect_expr(e, set),
        Stmt::Return(Some(e)) => collect_expr(e, set),
        Stmt::Block(stmts) => {
            for s in stmts {
                collect_stmt(s, set);
            }
        }
        Stmt::Goto(_) | Stmt::Label(_) | Stmt::Return(None) | Stmt::Line { .. } => {}
    }
}

fn collect_ty(ty: &CTy, set: &mut HashSet<String>) {
    match ty {
        CTy::Ident(name) => {
            set.insert(name.clone());
        }
        CTy::PtrTo(inner) => collect_ty(inner, set),
        CTy::Array { elem, .. } => collect_ty(elem, set),
        CTy::Struct { fields } => {
            for (ty, _) in fields {
                collect_ty(ty, set);
            }
        }
        _ => {}
    }
}

fn collect_expr(e: &Expr, set: &mut HashSet<String>) {
    match e {
        Expr::Int(_)
        | Expr::Long(_)
        | Expr::Float(_)
        | Expr::F32(_)
        | Expr::Null
        | Expr::Nan { .. }
        | Expr::Inf { .. }
        | Expr::LabelAddr(_)
        | Expr::CStr(_) => {}
        Expr::Ident(name) | Expr::Call { name, .. } => {
            set.insert(name.clone());
        }
        Expr::IndirectCall { callee, args } => {
            collect_expr(callee, set);
            for a in args {
                collect_expr(a, set);
            }
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Deref(expr)
        | Expr::AddrOf(expr)
        | Expr::PostInc(expr) => collect_expr(expr, set),
        Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs } => {
            collect_expr(lhs, set);
            collect_expr(rhs, set);
        }
        Expr::Ternary {
            cond,
            then_e,
            else_e,
        } => {
            collect_expr(cond, set);
            collect_expr(then_e, set);
            collect_expr(else_e, set);
        }
        Expr::Comma(a, b) => {
            collect_expr(a, set);
            collect_expr(b, set);
        }
        Expr::Compound(elems) | Expr::CompoundTyped { elems, .. } => {
            for el in elems {
                collect_expr(el, set);
            }
            if let Expr::CompoundTyped { ty, .. } = e {
                collect_ty(ty, set);
            }
        }
        Expr::Gnu { stmts, result } => {
            for s in stmts {
                collect_stmt(s, set);
            }
            collect_expr(result, set);
        }
    }
}

/// Rewrites positional local references (`l{n}`, param/decl names included) to their resolved
/// source names. Call targets, labels, globals (`g{n}`), builder temps (`t{n}`), and type names
/// live in separate namespaces and are left untouched.
fn rewrite_stmt(stmt: &mut Stmt, names: &[String]) {
    match stmt {
        Stmt::Expr(e) => rewrite_expr(e, names),
        Stmt::Assign { dest, src } => {
            rewrite_expr(dest, names);
            rewrite_expr(src, names);
        }
        Stmt::Decl { name, init, .. } => {
            rename_decl_name(name, names);
            if let Some(e) = init {
                rewrite_expr(e, names);
            }
        }
        Stmt::If {
            cond,
            then_s,
            else_s,
        } => {
            rewrite_expr(cond, names);
            rewrite_stmt(then_s, names);
            if let Some(s) = else_s {
                rewrite_stmt(s, names);
            }
        }
        Stmt::Switch { expr, arms } => {
            rewrite_expr(expr, names);
            for arm in arms {
                for s in &mut arm.body {
                    rewrite_stmt(s, names);
                }
            }
        }
        Stmt::For {
            init,
            cond,
            step,
            body,
        } => {
            rewrite_stmt(init, names);
            rewrite_expr(cond, names);
            rewrite_stmt(step, names);
            rewrite_stmt(body, names);
        }
        Stmt::GotoIndirect(e) => rewrite_expr(e, names),
        Stmt::Return(Some(e)) => rewrite_expr(e, names),
        Stmt::Block(stmts) => {
            for s in stmts {
                rewrite_stmt(s, names);
            }
        }
        Stmt::Goto(_) | Stmt::Label(_) | Stmt::Return(None) | Stmt::Line { .. } => {}
    }
}

fn rename_decl_name(name: &mut String, names: &[String]) {
    if let Some(idx) = parse_numbered(name, 'l') {
        if let Some(n) = names.get(idx) {
            *name = n.clone();
        }
    }
}

fn rewrite_expr(e: &mut Expr, names: &[String]) {
    match e {
        Expr::Int(_)
        | Expr::Long(_)
        | Expr::Float(_)
        | Expr::F32(_)
        | Expr::Null
        | Expr::Nan { .. }
        | Expr::Inf { .. }
        | Expr::LabelAddr(_)
        | Expr::CStr(_) => {}
        Expr::Ident(s) => {
            if let Some(idx) = parse_numbered(s, 'l') {
                if let Some(n) = names.get(idx) {
                    *s = n.clone();
                }
            }
        }
        Expr::Call { args, .. } => {
            for a in args {
                rewrite_expr(a, names);
            }
        }
        Expr::IndirectCall { callee, args } => {
            rewrite_expr(callee, names);
            for a in args {
                rewrite_expr(a, names);
            }
        }
        Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::Deref(expr)
        | Expr::AddrOf(expr)
        | Expr::PostInc(expr) => rewrite_expr(expr, names),
        Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs } => {
            rewrite_expr(lhs, names);
            rewrite_expr(rhs, names);
        }
        Expr::Ternary {
            cond,
            then_e,
            else_e,
        } => {
            rewrite_expr(cond, names);
            rewrite_expr(then_e, names);
            rewrite_expr(else_e, names);
        }
        Expr::Comma(a, b) => {
            rewrite_expr(a, names);
            rewrite_expr(b, names);
        }
        Expr::Compound(elems) | Expr::CompoundTyped { elems, .. } => {
            for el in elems {
                rewrite_expr(el, names);
            }
        }
        Expr::Gnu { stmts, result } => {
            for s in stmts {
                rewrite_stmt(s, names);
            }
            rewrite_expr(result, names);
        }
    }
}

fn parse_numbered(s: &str, prefix: char) -> Option<usize> {
    s.strip_prefix(prefix)?.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Local, LocalDecl};
    use dream_types::TypeInterner;

    use super::super::ast::{CTy, Param, Stmt};

    fn func(locals: &[Option<&str>], params: &[u32]) -> MirFunction {
        let interner = TypeInterner::new();
        MirFunction {
            def: dream_types::DefId(0),
            instance: vec![],
            name: "test_dream".into(),
            params: params.iter().map(|i| Local(*i)).collect(),
            ret: interner.int(),
            locals: locals
                .iter()
                .map(|n| LocalDecl {
                    ty: interner.int(),
                    name: n.map(|s| s.to_string()),
                    is_ref: false,
                    is_take: false,
                    is_cursor: false,
                    manual_drop: false,
                })
                .collect(),
            blocks: vec![],
            entry: crate::BlockId(0),
            is_async: false,
            hir_fn: None,
            file: None,
            prefer_inline: false,
        }
    }

    #[test]
    fn unnamed_locals_stay_positional() {
        let f = func(&[None, None], &[0]);
        assert_eq!(base_names(&f), vec!["l0".to_string(), "l1".to_string()]);
    }

    #[test]
    fn source_names_win_and_sanitize() {
        let f = func(&[Some("count"), Some("a-b")], &[0]);
        assert_eq!(
            base_names(&f),
            vec!["count".to_string(), "a_b".to_string()]
        );
    }

    #[test]
    fn duplicate_source_names_suffix() {
        let f = func(&[Some("v"), Some("v"), Some("v!")], &[]);
        assert_eq!(
            base_names(&f),
            vec!["v".to_string(), "v_2".to_string(), "v_".to_string()]
        );
    }

    #[test]
    fn synthetic_spellings_are_banned() {
        let f = func(&[Some("l3"), Some("t12"), Some("g7"), Some("__vs0")], &[]);
        assert_eq!(
            base_names(&f),
            vec![
                "l3_2".to_string(),
                "t12_2".to_string(),
                "g7_2".to_string(),
                "vs0".to_string()
            ]
        );
    }

    #[test]
    fn body_references_reserve_names() {
        let f = func(&[Some("memcpy"), Some("ok")], &[]);
        let body = vec![Stmt::call(
            "memcpy",
            vec![Expr::Null, Expr::Null, Expr::Int(0)],
        )];
        let names = resolve_names(&f, &body);
        assert_eq!(names[0], "memcpy_2");
        assert_eq!(names[1], "ok");
    }

    #[test]
    fn apply_rewrites_params_decls_and_refs() {
        let f = func(&[Some("total"), None], &[0]);
        let mut func_ast = Func {
            attr: None,
            export: None,
            static_: false,
            ret: CTy::I32,
            name: "test_dream".into(),
            params: vec![Param {
                ty: CTy::I32,
                name: "l0".into(),
            }],
            body: vec![
                Stmt::decl(CTy::I32, "l1", Some(Expr::local(0))),
                Stmt::Assign {
                    dest: Expr::local(1),
                    src: Expr::local(0),
                },
            ],
        };
        apply_local_names(&f, &mut func_ast);
        assert_eq!(func_ast.params[0].name, "total");
        match &func_ast.body[0] {
            Stmt::Decl { name, init, .. } => {
                assert_eq!(name, "l1");
                assert!(
                    matches!(init.as_ref(), Some(Expr::Ident(s)) if s == "total"),
                    "expected init `total`, got {:?}", init
                );
            }
            other => panic!("unexpected: {:?}", other),
        }
        match &func_ast.body[1] {
            Stmt::Assign { dest, src } => {
                assert!(
                    matches!(dest, Expr::Ident(s) if s == "l1"),
                    "expected dest `l1`, got {:?}", dest
                );
                assert!(
                    matches!(src, Expr::Ident(s) if s == "total"),
                    "expected src `total`, got {:?}", src
                );
            }
            other => panic!("unexpected: {:?}", other),
        }
    }
}
