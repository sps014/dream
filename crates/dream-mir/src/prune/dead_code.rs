//! The MIR reachability core: [`prune_module`] removes functions unreachable from the module's entry
//! points, then drops globals no surviving function reads.

use super::hir_edges::{hir_body_edges, HirEdges};
use super::FnKey;
use crate::lower;
use crate::{Global, Mir, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{TyKind, TypeId, TypeInterner};
use std::collections::{HashMap, HashSet};

/// Records every callable this rvalue statically references (direct calls, first-class function
/// refs, and user constructors) into `out`.
fn rvalue_callees(rv: &Rvalue, out: &mut Vec<FnKey>) {
    match rv {
        Rvalue::Call { callee, .. } | Rvalue::FuncRef(callee) | Rvalue::JsCall { callee, .. } => {
            out.push((callee.def, callee.args.clone()))
        }
        Rvalue::New {
            ctor: Some(ctor), ..
        } => out.push((*ctor, vec![])),
        _ => {}
    }
}

/// Removes functions unreachable from the module's entry points, then tree-shakes the module's other
/// symbol tables. Dead pure stores to never-read globals are removed (then the now-unreferenced
/// globals are dropped), layouts that no surviving function references are dropped (so protocol
/// strings / `$release_*` / marshalers are not emitted for the unused prelude), and unreferenced
/// `extern` imports are dropped. See [`prune_functions`] for the reachability core; the extra
/// shaking lives in [`prune_dead_globals`] / [`prune_dead_layouts`] / [`prune_dead_imports`].
pub fn prune_module(mir: &mut Mir, interner: &TypeInterner) {
    prune_functions(mir);
    prune_dead_globals(mir);
    prune_dead_layouts(mir, interner);
    prune_dead_imports(mir, interner);
    prune_dead_intrinsics(mir);
}

/// Collects every `DefId` named by a surviving direct call, `JsCall`, `FuncRef`, constructor, or
/// async HIR callee. Used to tree-shake host imports and `@intrinsic` keys.
fn live_callee_defs(mir: &Mir) -> HashSet<dream_types::DefId> {
    let mut live_defs: HashSet<dream_types::DefId> = HashSet::new();
    for f in &mir.functions {
        for b in &f.blocks {
            for s in &b.stmts {
                match s {
                    Statement::Call { callee, .. } | Statement::JsCall { callee, .. } => {
                        live_defs.insert(callee.def);
                    }
                    Statement::Assign(_, rv) => {
                        collect_import_defs_rvalue(rv, &mut live_defs);
                    }
                    _ => {}
                }
            }
            if let Terminator::TailCall { callee, .. } = &b.terminator {
                live_defs.insert(callee.def);
            }
        }
        if f.is_async {
            if let Some(hir_fn) = &f.hir_fn {
                let mut edges = HirEdges::default();
                hir_body_edges(&hir_fn.body, &mut edges);
                for (def, _) in edges.callees {
                    live_defs.insert(def);
                }
            }
        }
    }
    live_defs
}

/// Drops `@intrinsic` registry entries whose `DefId` is not called from surviving functions.
/// Linked runtimes (regex/PCRE2) key off this list; keeping prelude-wide keys would ingest them
/// into every `import system;` program.
fn prune_dead_intrinsics(mir: &mut Mir) {
    let live = live_callee_defs(mir);
    mir.intrinsics.retain(|(def, _)| live.contains(def));
}

/// Drops `mir.imports` whose `DefId` is not referenced by any surviving call / `JsCall` / `FuncRef`.
///
/// Generated struct↔js marshalers (emitted later as WAT) call the `js*` host bridges by symbol, so
/// whenever a surviving `Cast` involves `js` — or any `JsCall` remains — every import whose host
/// `field` starts with `js` is kept even if no MIR call edge names it. GPU-only modules keep
/// `jsRetain`/`jsRelease` so host handle RC stays bound.
fn prune_dead_imports(mir: &mut Mir, interner: &TypeInterner) {
    let live_defs = live_callee_defs(mir);

    let keep_js_bridges = module_uses_js_bridges(mir, interner);
    // GPU resources are `js` handles; `$release_js` calls `$js_release` even when no `js*` stdlib
    // bridge remains. Keep the stdlib `jsRetain`/`jsRelease` externs so the ABI/runtime bind them
    // (the emitter still replaces the WAT with compiler-emitted `$js_retain`/`$js_release`).
    let keep_js_rc = keep_js_bridges
        || mir
            .imports
            .iter()
            .any(|imp| live_defs.contains(&imp.def) && imp.field.starts_with("gpu"));
    mir.imports.retain(|imp| {
        // Generated `$Foo_to_js` / `$js_to_Foo` marshalers call `js*` bridges by symbol. Keep every
        // `js*` import only when a live `JsCall` or `js` Cast actually needs them — not merely
        // because some struct layout survived.
        live_defs.contains(&imp.def)
            || (imp.field.starts_with("js") && keep_js_bridges)
            || (keep_js_rc && (imp.field == "jsRetain" || imp.field == "jsRelease"))
    });
}

/// True when a surviving `JsCall` or a `Cast` involving `js` remains. Generated marshalers and the
/// `js*` host-bridge imports are only needed in that case (debug builds skip WAT DCE, so unused
/// marshalers would otherwise call dropped `$js_object` / `$js_array` imports).
pub(crate) fn module_uses_js_bridges(mir: &Mir, interner: &TypeInterner) -> bool {
    let is_js = |ty: TypeId| matches!(interner.kind(ty), TyKind::Js);
    for f in &mir.functions {
        for b in &f.blocks {
            for s in &b.stmts {
                match s {
                    Statement::JsCall { .. } => return true,
                    Statement::Assign(_, Rvalue::JsCall { .. }) => return true,
                    Statement::Assign(_, Rvalue::Cast(_, from, to))
                        if is_js(*from) || is_js(*to) =>
                    {
                        return true;
                    }
                    _ => {}
                }
            }
        }
    }
    false
}

/// Drops struct/union layouts that no surviving function, global, or live interface impl references.
///
/// `mir.layouts` is filled from the full analyzed prelude, so a `println` program otherwise interned
/// hundreds of `to_string` field-label strings and emitted `$release_*`/`$Type_to_string` for every
/// stdlib type. WAT DCE drops dead funcs but keeps every data segment, so unused layouts dominated
/// tiny `--release` binaries. Remaining layouts still drive protocol strings, release helpers, tags,
/// and JS marshalers.
fn prune_dead_layouts(mir: &mut Mir, interner: &TypeInterner) {
    let live = live_layout_types(mir, interner);
    mir.layouts.structs.retain(|ty, _| live.contains(ty));
    mir.layouts.unions.retain(|ty, _| live.contains(ty));
}

fn live_layout_types(mir: &Mir, interner: &TypeInterner) -> HashSet<TypeId> {
    let mut live: HashSet<TypeId> = HashSet::new();
    let mut work: Vec<TypeId> = Vec::new();
    let seed = |ty: TypeId, live: &mut HashSet<TypeId>, work: &mut Vec<TypeId>| {
        if live.insert(ty) {
            work.push(ty);
        }
    };

    for g in &mir.globals {
        seed(g.ty, &mut live, &mut work);
    }
    for f in &mir.functions {
        seed(f.ret, &mut live, &mut work);
        for ty in &f.instance {
            seed(*ty, &mut live, &mut work);
        }
        for l in &f.locals {
            seed(l.ty, &mut live, &mut work);
        }
        for b in &f.blocks {
            for s in &b.stmts {
                collect_stmt_types(s, &mut |ty| seed(ty, &mut live, &mut work));
            }
        }
        if f.is_async {
            if let Some(hir_fn) = &f.hir_fn {
                let mut edges = HirEdges::default();
                hir_body_edges(&hir_fn.body, &mut edges);
                for ty in edges.types {
                    seed(ty, &mut live, &mut work);
                }
            }
        }
    }

    let kept_names: HashSet<&str> = mir.functions.iter().map(|f| f.name.as_str()).collect();
    for imp in &mir.interfaces.impls {
        if imp
            .entries
            .iter()
            .any(|(_, syms)| syms.iter().any(|s| kept_names.contains(s.as_str())))
        {
            seed(imp.class_ty, &mut live, &mut work);
        }
    }

    while let Some(ty) = work.pop() {
        let nested: Vec<TypeId> = match interner.kind(ty) {
            TyKind::Array(elem) => vec![*elem],
            TyKind::Struct(_, args) | TyKind::Union(_, args) | TyKind::Interface(_, args) => {
                args.clone()
            }
            TyKind::Func(params, ret) => {
                let mut v = params.clone();
                v.push(*ret);
                v
            }
            TyKind::Tuple(elems) => elems.clone(),
            TyKind::Prim(_)
            | TyKind::Object
            | TyKind::Void
            | TyKind::Error
            | TyKind::Enum(_)
            | TyKind::Js => Vec::new(),
        };
        for n in nested {
            seed(n, &mut live, &mut work);
        }
        if let Some(l) = mir.layouts.structs.get(&ty) {
            for f in &l.fields {
                seed(f.ty, &mut live, &mut work);
            }
        }
        if let Some(l) = mir.layouts.unions.get(&ty) {
            for v in &l.variants {
                for f in &v.fields {
                    seed(f.ty, &mut live, &mut work);
                }
            }
        }
    }
    live
}

fn collect_stmt_types(s: &Statement, seed: &mut impl FnMut(TypeId)) {
    match s {
        Statement::Assign(_, rv) => collect_rvalue_types(rv, seed),
        Statement::Call { callee, .. } => collect_callee_types(callee, seed),
        Statement::JsCall { callee, args, .. } => {
            collect_callee_types(callee, seed);
            for (_, ty) in args {
                seed(*ty);
            }
        }
        Statement::InterfaceCall { sig, .. } => seed(*sig),
        Statement::IndirectCall { sig, .. } => seed(*sig),
        Statement::Print { ty, .. }
        | Statement::ArrayElemsCopy { elem_ty: ty, .. }
        | Statement::ArrayElemsFill { elem_ty: ty, .. } => seed(*ty),
        Statement::Retain(_)
        | Statement::Release(_)
        | Statement::ReleaseUnique(_)
        | Statement::Panic(_)
        | Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::ForceFree(_)
        | Statement::LockAcquire(_)
        | Statement::LockRelease(_)
        | Statement::DeferEnter
        | Statement::RegionEnter
        | Statement::RegionLeave
        | Statement::DeferLeave(_)
        | Statement::SimdV128 { .. }
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => {}
    }
}

fn collect_callee_types(callee: &crate::Callee, seed: &mut impl FnMut(TypeId)) {
    for a in &callee.args {
        seed(*a);
    }
    seed(callee.ret);
}

fn collect_rvalue_types(rv: &Rvalue, seed: &mut impl FnMut(TypeId)) {
    match rv {
        Rvalue::New { ty, .. }
        | Rvalue::UnionNew { ty, .. }
        | Rvalue::Tuple { ty, .. }
        | Rvalue::ToBytes { ty, .. }
        | Rvalue::FromBytes { ty, .. }
        | Rvalue::UnionField { ty, .. }
        | Rvalue::IsType(_, ty) => seed(*ty),
        Rvalue::ArrayNew { elem_ty, .. }
        | Rvalue::ArrayLit { elem_ty, .. }
        | Rvalue::ArrayRealloc { elem_ty, .. } => seed(*elem_ty),
        Rvalue::Cast(_, from, to) => {
            seed(*from);
            seed(*to);
        }
        Rvalue::Call { callee, .. } | Rvalue::FuncRef(callee) => collect_callee_types(callee, seed),
        Rvalue::JsCall { callee, args, .. } => {
            collect_callee_types(callee, seed);
            for (_, ty) in args {
                seed(*ty);
            }
        }
        Rvalue::IndirectCall { sig, .. } => seed(*sig),
        Rvalue::InterfaceCall { sig, ret, .. } => {
            seed(*sig);
            seed(*ret);
        }
        Rvalue::Use(_)
        | Rvalue::Select { .. }
        | Rvalue::Binary(_, _, _)
        | Rvalue::Unary(_, _)
        | Rvalue::StrLen(_)
        | Rvalue::StrByteSize(_)
        | Rvalue::CharAt(_, _, _)
        | Rvalue::ByteAt(_, _, _)
        | Rvalue::HashCode(_)
        | Rvalue::ToString(_)
        | Rvalue::Concat(_)
        | Rvalue::ConcatInt { .. }
        | Rvalue::EnumName { .. }
        | Rvalue::ArrayLen(_)
        | Rvalue::Discriminant { .. } => {}
    }
}

fn collect_import_defs_rvalue(rv: &Rvalue, out: &mut HashSet<dream_types::DefId>) {
    match rv {
        Rvalue::Call { callee, .. } | Rvalue::FuncRef(callee) | Rvalue::JsCall { callee, .. } => {
            out.insert(callee.def);
        }
        Rvalue::New {
            ctor: Some(ctor), ..
        } => {
            out.insert(*ctor);
        }
        _ => {}
    }
}

/// Removes functions unreachable from the module's entry points (the reachability core of
/// [`prune_module`]).
///
/// Reachability starts from `main` and the synthesized global initializer and follows direct calls
/// (including [`Terminator::TailCall`]), `FuncRef`s, and constructors. An `IndirectCall` has no
/// static target, but its only possible targets are functions whose address was taken by a
/// `FuncRef` in reachable code — which the `FuncRef` edges already keep — so the result stays sound.
fn prune_functions(mir: &mut Mir) {
    let index: HashMap<FnKey, usize> = mir
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.def, f.instance.clone()), i))
        .collect();

    // `<Type>_del`/`<Type>_to_string` are invoked only by the generated RC runtime (the release
    // helpers and `$print_object`), never by a normal call edge, so reachability tracks them by name
    // for every type that is *live* — constructed (`New`/`UnionNew`) or printed — plus, transitively,
    // the types of its (reference) fields, whose release/print the runtime chains into.
    let by_name: HashMap<&str, usize> = mir
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    let mut reachable: HashSet<usize> = HashSet::new();
    let mut live_types: HashSet<TypeId> = HashSet::new();
    let mut type_worklist: Vec<TypeId> = Vec::new();
    let mut worklist: Vec<usize> = mir
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| f.name == crate::abi::ENTRY_FN || f.name == lower::INIT_FN_NAME)
        .map(|(i, _)| i)
        .collect();

    loop {
        while let Some(idx) = worklist.pop() {
            if !reachable.insert(idx) {
                continue;
            }
            let mut callees = Vec::new();
            let mut iface_uses: Vec<(usize, usize)> = Vec::new();
            for block in &mir.functions[idx].blocks {
                for stmt in &block.stmts {
                    match stmt {
                        Statement::Call { callee, .. } => {
                            callees.push((callee.def, callee.args.clone()))
                        }
                        Statement::JsCall { callee, .. } => {
                            callees.push((callee.def, callee.args.clone()))
                        }
                        Statement::InterfaceCall {
                            iface_id,
                            method_slot,
                            ..
                        } => iface_uses.push((*iface_id, *method_slot)),
                        Statement::Assign(_, rv) => {
                            rvalue_callees(rv, &mut callees);
                            if let Rvalue::InterfaceCall {
                                iface_id,
                                method_slot,
                                ..
                            } = rv
                            {
                                iface_uses.push((*iface_id, *method_slot));
                            }
                            match rv {
                                Rvalue::New { ty, .. }
                                | Rvalue::UnionNew { ty, .. }
                                | Rvalue::Tuple { ty, .. } => type_worklist.push(*ty),
                                _ => {}
                            }
                        }
                        Statement::Print { ty, .. } => type_worklist.push(*ty),
                        _ => {}
                    }
                }
                // TCO rewrites call+return into `TailCall` after module prune today, but walk it
                // anyway so a future pass reordering cannot drop a still-reachable callee.
                if let Terminator::TailCall { callee, .. } = &block.terminator {
                    callees.push((callee.def, callee.args.clone()));
                }
            }
            // An async function's MIR body is a stub; its real call/type edges live in the preserved
            // HIR snapshot, so walk that too (otherwise awaited helpers would be pruned).
            let f = &mir.functions[idx];
            if f.is_async {
                if let Some(hir_fn) = &f.hir_fn {
                    let mut edges = HirEdges::default();
                    hir_body_edges(&hir_fn.body, &mut edges);
                    callees.extend(edges.callees);
                    type_worklist.extend(edges.types);
                    iface_uses.extend(edges.iface_calls);
                }
            }
            for key in callees {
                if let Some(&target) = index.get(&key) {
                    if !reachable.contains(&target) {
                        worklist.push(target);
                    }
                }
            }
            // An interface call may dynamically reach the concrete method of *any* class that
            // implements that interface. Keep each such `{Class}_{method}` implementation alive
            // (by name, like the RC-runtime-only `_del`/`_to_string` helpers).
            for (iface_id, slot) in iface_uses {
                for imp in &mir.interfaces.impls {
                    for (id, symbols) in &imp.entries {
                        if *id == iface_id {
                            if let Some(sym) = symbols.get(slot) {
                                if let Some(&t) = by_name.get(sym.as_str()) {
                                    if !reachable.contains(&t) {
                                        worklist.push(t);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Expand the live-type frontier: keep each type's destructor/`to_string` and recurse into its
        // fields. Any newly-kept function is pushed back so its own body is walked; the outer loop
        // reaches a fixpoint once the function worklist drains and no new type is discovered.
        while let Some(ty) = type_worklist.pop() {
            if !live_types.insert(ty) {
                continue;
            }
            let mut field_tys = Vec::new();
            let mut names = Vec::new();
            if let Some(l) = mir.layouts.structs.get(&ty) {
                names.push(l.name.clone());
                field_tys.extend(l.fields.iter().map(|f| f.ty));
            }
            if let Some(l) = mir.layouts.unions.get(&ty) {
                names.push(l.name.clone());
                field_tys.extend(
                    l.variants
                        .iter()
                        .flat_map(|v| v.fields.iter().map(|f| f.ty)),
                );
            }
            for name in names {
                for sym in [format!("{}_del", name), format!("{}_to_string", name)] {
                    if let Some(&idx) = by_name.get(sym.as_str()) {
                        if !reachable.contains(&idx) {
                            worklist.push(idx);
                        }
                    }
                }
            }
            type_worklist.extend(field_tys);
        }
        if worklist.is_empty() {
            break;
        }
    }
    drop(by_name);

    let mut keep = reachable.into_iter().collect::<Vec<_>>();
    keep.sort_unstable();
    let mut kept = Vec::with_capacity(keep.len());
    let mut kept_polls = Vec::new();
    let mut poll_iter = std::mem::take(&mut mir.polls).into_iter();
    for (i, f) in std::mem::take(&mut mir.functions).into_iter().enumerate() {
        let is_async = f.is_async;
        let poll = if is_async { poll_iter.next() } else { None };
        if keep.binary_search(&i).is_ok() {
            kept.push(f);
            if let Some(p) = poll {
                kept_polls.push(p);
            }
        }
    }
    mir.functions = kept;
    mir.polls = kept_polls;
}

/// Drops module globals that no surviving function reads. A global whose only writes are pure (no
/// call/allocation on the RHS) and which is never read is fully dead: its stores are removed and the
/// slot is dropped. A global written by an impure store (a call that may have side effects) is kept
/// even if never read, so the effect still runs. Globals are keyed by their stable `Global` id (the
/// backend emits `$g{id}` by id, not by position), so dropping entries never renumbers survivors.
fn prune_dead_globals(mir: &mut Mir) {
    let mut read: HashSet<Global> = HashSet::new();
    for f in mir.functions.iter().chain(mir.polls.iter()) {
        for b in &f.blocks {
            for s in &b.stmts {
                collect_global_reads_stmt(s, &mut read);
            }
            collect_global_reads_terminator(&b.terminator, &mut read);
        }
    }
    // Remove pure stores to never-read globals.
    for f in mir.functions.iter_mut().chain(mir.polls.iter_mut()) {
        for b in &mut f.blocks {
            b.stmts.retain(|s| match s {
                Statement::Assign(Place::Global(g), rv) => {
                    read.contains(g) || !crate::passes::is_pure(rv)
                }
                _ => true,
            });
        }
    }
    // A global stays if it is still read or still written by a surviving (impure) store.
    let mut referenced = read;
    for f in mir.functions.iter().chain(mir.polls.iter()) {
        for b in &f.blocks {
            for s in &b.stmts {
                if let Statement::Assign(Place::Global(g), _) = s {
                    referenced.insert(*g);
                }
            }
        }
    }
    // `Global(0)` is always the synthetic `__closure_env` slot (`register_globals` registers it
    // first, unconditionally, before any user global — see its doc comment). The unconditionally
    // emitted `$__dream_worker_invoke` trampoline (`src/mir/emit/module.rs`) writes to it by literal
    // id (`$g0`) even in a program with no closures at all, so it must never be pruned even though
    // no *MIR function* reads or writes it in that case.
    referenced.insert(Global(0));
    mir.globals.retain(|g| referenced.contains(&g.id));
}

fn collect_global_reads_stmt(s: &Statement, out: &mut HashSet<Global>) {
    match s {
        Statement::Assign(place, rv) => {
            if let Place::Index { index, .. } = place {
                collect_global_reads_operand(index, out);
            }
            collect_global_reads_rvalue(rv, out);
        }
        Statement::Retain(o) | Statement::Release(o) | Statement::ReleaseUnique(o) | Statement::Panic(o) => {
            collect_global_reads_operand(o, out)
        }
        Statement::Call { args, .. } => args
            .iter()
            .for_each(|a| collect_global_reads_operand(a, out)),
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            collect_global_reads_operand(target, out);
            if let Some(v) = via {
                collect_global_reads_operand(v, out);
            }
            if let Some(m) = method {
                collect_global_reads_operand(m, out);
            }
            args.iter()
                .for_each(|(a, _)| collect_global_reads_operand(a, out));
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            collect_global_reads_operand(receiver, out);
            args.iter()
                .for_each(|a| collect_global_reads_operand(a, out));
        }
        Statement::IndirectCall { target, args, .. } => {
            collect_global_reads_operand(target, out);
            args.iter()
                .for_each(|a| collect_global_reads_operand(a, out));
        }
        Statement::Print { arg, .. } => collect_global_reads_operand(arg, out),
        Statement::ForceFree(o) => collect_global_reads_operand(o, out),
        Statement::ValueDrop(_) | Statement::ValueRetain(_) | Statement::ValueKill(_) => {}
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            collect_global_reads_operand(dst, out);
            collect_global_reads_operand(dst_off, out);
            collect_global_reads_operand(src, out);
            collect_global_reads_operand(src_off, out);
            collect_global_reads_operand(count, out);
        }
        Statement::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => {
            collect_global_reads_operand(dst, out);
            collect_global_reads_operand(dst_off, out);
            collect_global_reads_operand(count, out);
        }
        Statement::LockAcquire(o) | Statement::LockRelease(o) | Statement::DeferLeave(o) => {
            collect_global_reads_operand(o, out)
        }
        Statement::DeferEnter | Statement::RegionEnter | Statement::RegionLeave => {}
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ..
        } => {
            collect_global_reads_operand(dest, out);
            collect_global_reads_operand(lhs, out);
            collect_global_reads_operand(rhs, out);
            collect_global_reads_operand(index, out);
            if let Some(s) = splat_rhs {
                collect_global_reads_operand(s, out);
            }
        }
        Statement::Nop | Statement::DebugLine(_) | Statement::SourceLine(_) => {}
    }
}

fn collect_global_reads_rvalue(rv: &Rvalue, out: &mut HashSet<Global>) {
    match rv {
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            collect_global_reads_operand(cond, out);
            collect_global_reads_operand(then_val, out);
            collect_global_reads_operand(else_val, out);
        }
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::ArrayLen(o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::Cast(o, _, _)
        | Rvalue::IsType(o, _)
        | Rvalue::Discriminant { base: o, .. }
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::EnumName { value: o, .. }
        | Rvalue::ArrayNew { len: o, .. }
        | Rvalue::ToBytes { value: o, .. }
        | Rvalue::FromBytes { bytes: o, .. }
        | Rvalue::UnionField { base: o, .. } => collect_global_reads_operand(o, out),
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            collect_global_reads_operand(array, out);
            collect_global_reads_operand(new_len, out);
        }
        Rvalue::Binary(_, a, b) | Rvalue::CharAt(a, b, _) | Rvalue::ByteAt(a, b, _) => {
            collect_global_reads_operand(a, out);
            collect_global_reads_operand(b, out);
        }
        Rvalue::Concat(parts) => {
            for p in parts {
                collect_global_reads_operand(p, out);
            }
        }
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => {
            collect_global_reads_operand(prefix, out);
            collect_global_reads_operand(value, out);
            collect_global_reads_operand(suffix, out);
        }
        Rvalue::Call { args, .. }
        | Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::ArrayLit { elems: args, .. }
        | Rvalue::Tuple { elems: args, .. } => args
            .iter()
            .for_each(|a| collect_global_reads_operand(a, out)),
        Rvalue::IndirectCall { target, args, .. } => {
            collect_global_reads_operand(target, out);
            args.iter()
                .for_each(|a| collect_global_reads_operand(a, out));
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            collect_global_reads_operand(receiver, out);
            args.iter()
                .for_each(|a| collect_global_reads_operand(a, out));
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            collect_global_reads_operand(target, out);
            if let Some(v) = via {
                collect_global_reads_operand(v, out);
            }
            if let Some(m) = method {
                collect_global_reads_operand(m, out);
            }
            args.iter()
                .for_each(|(a, _)| collect_global_reads_operand(a, out));
        }
        Rvalue::FuncRef(_) => {}
    }
}

fn collect_global_reads_terminator(t: &Terminator, out: &mut HashSet<Global>) {
    match t {
        Terminator::If { cond, .. } => collect_global_reads_operand(cond, out),
        Terminator::Switch { value, .. } => collect_global_reads_operand(value, out),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => {
            collect_global_reads_operand(o, out)
        }
        Terminator::TailCall { args, .. } => args
            .iter()
            .for_each(|a| collect_global_reads_operand(a, out)),
        _ => {}
    }
}

fn collect_global_reads_operand(op: &Operand, out: &mut HashSet<Global>) {
    if let Operand::Copy(place) = op {
        match place {
            Place::Global(g) => {
                out.insert(*g);
            }
            Place::Index { index, .. } => collect_global_reads_operand(index, out),
            Place::Local(_) | Place::Field { .. } | Place::Deref { .. } => {}
        }
    }
}
