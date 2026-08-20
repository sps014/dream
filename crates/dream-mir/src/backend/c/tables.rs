use crate::abi::TAG_STRUCT_BASE;
use crate::backend::c::types::c_ident;
use crate::backend::shared::func_symbol;
use crate::{Const, Mir, Operand, Place, Rvalue, Statement, Terminator};
use dream_types::{DefId, TypeId};
use indexmap::IndexMap;
use std::collections::HashMap;

pub(super) fn symbol_table(mir: &Mir) -> HashMap<(DefId, Vec<TypeId>), String> {
    let mut table: HashMap<(DefId, Vec<TypeId>), String> = mir
        .functions
        .iter()
        .map(|f| ((f.def, f.instance.clone()), c_ident(&func_symbol(f))))
        .collect();
    for imp in &mir.imports {
        table.insert(
            (imp.def, vec![]),
            crate::backend::c::types::import_call_name(mir, imp),
        );
    }
    for (def, key) in &mir.intrinsics {
        table.entry((*def, vec![])).or_insert_with(|| c_ident(key));
    }
    table
}

pub(super) fn struct_tags(mir: &Mir) -> HashMap<TypeId, i32> {
    mir.layouts
        .structs
        .keys()
        .chain(mir.layouts.unions.keys())
        .enumerate()
        .map(|(i, ty)| (*ty, TAG_STRUCT_BASE + i as i32))
        .collect()
}

pub(super) fn intern_strings(
    mir: &Mir,
    interner: &dream_types::TypeInterner,
) -> IndexMap<String, String> {
    let mut found = Vec::new();
    for f in &mir.functions {
        scan_func(f, &mut found);
        if f.is_async {
            if let Some(hir) = f.hir_fn.as_ref() {
                let body = crate::lower::lower_async_poll_body(hir, interner);
                scan_func(&body, &mut found);
            }
        }
    }
    for s in protocol_strings(mir) {
        found.push(s);
    }
    for base in crate::backend::shared::panic_msgs::ALL {
        found.push(base.to_string());
    }
    let mut map = IndexMap::new();
    let mut n = 0usize;
    for s in found {
        if !map.contains_key(&s) {
            map.insert(s, format!("__ds{n}"));
            n += 1;
        }
    }
    map
}

fn scan_func(f: &crate::MirFunction, found: &mut Vec<String>) {
    for b in &f.blocks {
        for s in &b.stmts {
            strings_in_stmt(s, found);
        }
        strings_in_term(&b.terminator, found);
    }
}

fn protocol_strings(mir: &Mir) -> Vec<String> {
    let mut v = vec![
        "null".into(),
        "<object>".into(),
        "[".into(),
        "]".into(),
        ", ".into(),
        "(".into(),
        ")".into(),
        "true".into(),
        "false".into(),
        "-".into(),
        "".into(),
    ];
    v.push("length".into());
    for (_ty, layout) in &mir.layouts.structs {
        v.push(format!("{} {{ ", layout.name));
        for (i, f) in layout.fields.iter().enumerate() {
            v.push(if i == 0 {
                format!("{}: ", f.name)
            } else {
                format!(", {}: ", f.name)
            });
            v.push(f.name.clone());
        }
        v.push(" }".into());
    }
    for layout in mir.layouts.unions.values() {
        for variant in &layout.variants {
            if variant.fields.is_empty() {
                v.push(variant.name.clone());
            } else {
                v.push(format!("{}(", variant.name));
                for (i, f) in variant.fields.iter().enumerate() {
                    v.push(if i == 0 {
                        format!("{}: ", f.name)
                    } else {
                        format!(", {}: ", f.name)
                    });
                }
                v.push(")".into());
            }
        }
    }
    v
}

fn strings_in_stmt(s: &Statement, out: &mut Vec<String>) {
    match s {
        Statement::Assign(_, rv) => strings_in_rv(rv, out),
        Statement::Call { args, .. } | Statement::IndirectCall { args, .. } => {
            for a in args {
                strings_in_op(a, out);
            }
        }
        Statement::InterfaceCall { receiver, args, .. } => {
            strings_in_op(receiver, out);
            for a in args {
                strings_in_op(a, out);
            }
        }
        Statement::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            strings_in_op(target, out);
            if let Some(v) = via {
                strings_in_op(v, out);
            }
            if let Some(m) = method {
                strings_in_op(m, out);
            }
            for (a, _) in args {
                strings_in_op(a, out);
            }
        }
        Statement::Print { arg, .. }
        | Statement::Panic(arg)
        | Statement::Retain(arg)
        | Statement::Release(arg)
        | Statement::ReleaseUnique(arg)
        | Statement::ForceFree(arg)
        | Statement::LockAcquire(arg)
        | Statement::LockRelease(arg)
        | Statement::DeferLeave(arg) => strings_in_op(arg, out),
        Statement::DeferEnter => {}
        Statement::ArrayElemsCopy {
            dst,
            dst_off,
            src,
            src_off,
            count,
            ..
        } => {
            strings_in_op(dst, out);
            strings_in_op(dst_off, out);
            strings_in_op(src, out);
            strings_in_op(src_off, out);
            strings_in_op(count, out);
        }
        Statement::ArrayElemsFill {
            dst,
            dst_off,
            count,
            ..
        } => {
            strings_in_op(dst, out);
            strings_in_op(dst_off, out);
            strings_in_op(count, out);
        }
        Statement::SimdV128 {
            dest,
            lhs,
            rhs,
            index,
            splat_rhs,
            ..
        } => {
            strings_in_op(dest, out);
            strings_in_op(lhs, out);
            strings_in_op(rhs, out);
            strings_in_op(index, out);
            if let Some(s) = splat_rhs {
                strings_in_op(s, out);
            }
        }
        Statement::Nop
        | Statement::DebugLine(_)
        | Statement::SourceLine(_)
        | Statement::ValueDrop(_)
        | Statement::ValueRetain(_)
        | Statement::ValueKill(_) => {}
    }
}

fn strings_in_term(t: &Terminator, out: &mut Vec<String>) {
    match t {
        Terminator::If { cond, .. } => strings_in_op(cond, out),
        Terminator::Switch { value, .. } => strings_in_op(value, out),
        Terminator::Return(Some(o)) | Terminator::AsyncComplete(Some(o)) => strings_in_op(o, out),
        Terminator::TailCall { args, .. } => {
            for a in args {
                strings_in_op(a, out);
            }
        }
        Terminator::Await { future, .. } => strings_in_op(future, out),
        Terminator::Goto(_)
        | Terminator::Return(None)
        | Terminator::AsyncComplete(None)
        | Terminator::Unreachable => {}
    }
}

fn strings_in_rv(rv: &Rvalue, out: &mut Vec<String>) {
    match rv {
        Rvalue::Use(o)
        | Rvalue::Unary(_, o)
        | Rvalue::StrLen(o)
        | Rvalue::StrByteSize(o)
        | Rvalue::HashCode(o)
        | Rvalue::ToString(o)
        | Rvalue::ArrayLen(o)
        | Rvalue::Discriminant(o)
        | Rvalue::IsType(o, _) => strings_in_op(o, out),
        Rvalue::Select {
            cond,
            then_val,
            else_val,
        } => {
            strings_in_op(cond, out);
            strings_in_op(then_val, out);
            strings_in_op(else_val, out);
        }
        Rvalue::Binary(_, a, b) | Rvalue::CharAt(a, b, _) | Rvalue::ByteAt(a, b, _) => {
            strings_in_op(a, out);
            strings_in_op(b, out);
        }
        Rvalue::Concat(parts) => {
            for p in parts {
                strings_in_op(p, out);
            }
        }
        Rvalue::ConcatInt {
            prefix,
            value,
            suffix,
        } => {
            strings_in_op(prefix, out);
            strings_in_op(value, out);
            strings_in_op(suffix, out);
        }
        Rvalue::EnumName { value, arms } => {
            strings_in_op(value, out);
            for (_, n) in arms {
                out.push(n.clone());
            }
        }
        Rvalue::Call { args, .. } | Rvalue::IndirectCall { args, .. } => {
            for a in args {
                strings_in_op(a, out);
            }
        }
        Rvalue::InterfaceCall { receiver, args, .. } => {
            strings_in_op(receiver, out);
            for a in args {
                strings_in_op(a, out);
            }
        }
        Rvalue::JsCall {
            target,
            via,
            method,
            args,
            ..
        } => {
            strings_in_op(target, out);
            if let Some(v) = via {
                strings_in_op(v, out);
            }
            if let Some(m) = method {
                strings_in_op(m, out);
            }
            for (a, _) in args {
                strings_in_op(a, out);
            }
        }
        Rvalue::New { args, .. }
        | Rvalue::UnionNew { args, .. }
        | Rvalue::Tuple { elems: args, .. }
        | Rvalue::ArrayLit { elems: args, .. } => {
            for a in args {
                strings_in_op(a, out);
            }
        }
        Rvalue::ArrayNew { len, .. } => strings_in_op(len, out),
        Rvalue::ToBytes { value, .. } | Rvalue::FromBytes { bytes: value, .. } => {
            strings_in_op(value, out)
        }
        Rvalue::ArrayRealloc { array, new_len, .. } => {
            strings_in_op(array, out);
            strings_in_op(new_len, out);
        }
        Rvalue::Cast(o, _, _) => strings_in_op(o, out),
        Rvalue::UnionField { base, .. } => strings_in_op(base, out),
        Rvalue::FuncRef(_) => {}
    }
}

fn strings_in_op(o: &Operand, out: &mut Vec<String>) {
    match o {
        Operand::Const(Const::Str(s)) => out.push(s.clone()),
        Operand::Copy(Place::Index { index, .. }) => strings_in_op(index, out),
        Operand::Copy(_) | Operand::Const(_) => {}
    }
}
