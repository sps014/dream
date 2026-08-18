use crate::backend::c::ctx::Cx;
use crate::backend::c::protocol::{emit_iface_init, emit_iface_trampolines, emit_protocol};
use crate::backend::c::release::emit_release_helpers;
use crate::backend::c::statements::emit_stmt;
use crate::backend::c::terminator::emit_term;
use crate::backend::c::types::{c_ident, c_ty, local_c_ty};
use crate::backend::wasm::func_symbol;
use crate::passes::MirPass;
use crate::{Mir, MirFunction};
use dream_types::{TyKind, TypeInterner};

pub fn emit_c_module(mir: &Mir, interner: &TypeInterner) -> String {
    let cx = Cx::new(mir, interner);
    let mut out = String::new();
    out.push_str("#include \"dream_rt_native.h\"\n");
    out.push_str("#include <math.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>\n\n");
    emit_string_table(&mut out, &cx);
    emit_globals(&mut out, &cx);
    emit_imports(&mut out, &cx);
    let async_n = mir.functions.iter().filter(|f| f.is_async).count();
    emit_ftable_decl(&mut out, &cx, async_n);
    for f in &mir.functions {
        out.push_str(&format!("{};\n", proto(&cx, f)));
        if f.is_async {
            out.push_str(&format!("int32_t {}(dream_ptr __self);\n", poll_name(f)));
        }
    }
    out.push('\n');
    emit_release_helpers(&mut out, &cx);
    emit_protocol(&mut out, &cx);
    emit_iface_trampolines(&mut out, &cx);
    let mut async_i = 0usize;
    for f in &mir.functions {
        if f.is_async {
            let poll_idx = mir.functions.len() + 1 + async_i;
            emit_async_pair(&mut out, &cx, f, poll_idx as i32);
            async_i += 1;
        } else {
            emit_func(&mut out, &cx, f);
        }
    }
    emit_ftable_def(&mut out, &cx, async_n);
    out.push_str("void *dream_ft_get(int32_t i) {\n");
    out.push_str(&format!(
        "  return (i > 0 && i < {}) ? dream_ft[i] : 0;\n}}\n\n",
        mir.functions.len() + 1 + async_n
    ));
    emit_iface_init(&mut out, &cx);
    emit_worker_invoke(&mut out, &cx);
    if let Some(main) = mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)
    {
        out.push_str("int dream_guest_entry(void) {\n  dream_init_ft();\n  dream_init_itables();\n  dream_host_bind(dream_string_alloc, dream_array_new);\n");
        if let Some(init) = mir.functions.iter().find(|f| f.name == "__dream_init") {
            out.push_str(&format!("  {}();\n", c_ident(&func_symbol(init))));
        }
        if main.params.is_empty() {
            out.push_str("  main_dream();\n");
        } else {
            out.push_str("  main_dream(dream_array_new(0, 8));\n");
        }
        if async_n > 0 {
            out.push_str("  dream_run_loop();\n");
        }
        out.push_str("  return 0;\n}\n");
        out.push_str("int main(void) { return dream_guest_entry(); }\n");
    }
    out
}

fn emit_string_table(out: &mut String, cx: &Cx<'_>) {
    for (s, sym) in &cx.strings {
        let units: Vec<u16> = s.encode_utf16().collect();
        let n = units.len();
        out.push_str(&format!(
            "static struct {{ int32_t size, header_pad, tag, rc, len, pad; uint16_t u[{}]; }} {sym}_blk = {{\n",
            n.max(1)
        ));
        out.push_str(&format!(
            "  0, 0, TAG_STRING, INT32_MAX, {n}, 0, {{ {} }}\n}};\n",
            if units.is_empty() {
                "0".into()
            } else {
                units
                    .iter()
                    .map(|u| u.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
        out.push_str(&format!(
            "static const dream_ptr {sym} = (dream_ptr)((char *)&{sym}_blk + 16);\n\n"
        ));
    }
}

fn emit_globals(out: &mut String, cx: &Cx<'_>) {
    for g in &cx.mir.globals {
        if g.id.0 == 0 {
            out.push_str("_Thread_local dream_ptr g0 = 0;\n");
            continue;
        }
        if cx.interner.is_value_type(g.ty) {
            let size = crate::backend::c::types::elem_size(cx, g.ty).max(1);
            out.push_str(&format!(
                "_Alignas(8) static unsigned char __vg{}[{size}];\ndream_ptr g{} = (dream_ptr)(uintptr_t)__vg{};\n",
                g.id.0, g.id.0, g.id.0
            ));
            continue;
        }
        let ty = c_ty(cx.interner, g.ty);
        out.push_str(&format!("{ty} g{} = 0;\n", g.id.0));
    }
    if !cx.mir.globals.is_empty() {
        out.push('\n');
    }
}

fn emit_worker_invoke(out: &mut String, cx: &Cx<'_>) {
    let has_env = cx.mir.globals.iter().any(|g| g.id.0 == 0);
    out.push_str("dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg) {\n");
    out.push_str("  if (fn <= 0) return 0;\n");
    if has_env {
        out.push_str("  g0 = env;\n");
    } else {
        out.push_str("  (void)env;\n");
    }
    out.push_str("  return ((dream_fn)dream_ft[fn])(arg, 0, 0, 0, 0, 0, 0, 0);\n}\n\n");
}

fn emit_imports(out: &mut String, cx: &Cx<'_>) {
    for imp in &cx.mir.imports {
        let host = crate::backend::c::types::import_host_name(imp);
        let name = crate::backend::c::types::import_call_name(cx.mir, imp);
        let async_wrap = crate::backend::c::types::import_is_async_future(cx.mir, imp);
        let host_ret = if async_wrap {
            match cx.interner.kind(imp.ret.unwrap()) {
                TyKind::Struct(_, args) => match args.first() {
                    Some(t) if matches!(cx.interner.kind(*t), TyKind::Void) => "int32_t",
                    Some(t) => c_ty(cx.interner, *t),
                    None => "int32_t",
                },
                _ => "int32_t",
            }
        } else {
            imp.ret.map(|t| c_ty(cx.interner, t)).unwrap_or("void")
        };
        let ret = host_ret;
        let params: Vec<String> = imp
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{} a{i}", c_ty(cx.interner, *t)))
            .collect();
        let args = if params.is_empty() {
            "void".into()
        } else {
            params.join(", ")
        };
        let call_args: String = (0..imp.params.len())
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("{ret} {host}({args});\n"));
        if async_wrap {
            out.push_str(&format!("dream_ptr {name}({args}) {{\n"));
            out.push_str("  dream_ptr __f = dream_new_future(64, -1, 1);\n");
            if ret == "void" {
                out.push_str(&format!(
                    "  {host}({call_args});\n  dream_async_complete(__f, 0);\n"
                ));
            } else {
                out.push_str(&format!(
                    "  dream_async_complete(__f, (dream_ptr)(intptr_t){host}({call_args}));\n"
                ));
            }
            out.push_str("  return __f;\n}\n");
        } else if !matches!(
            host.as_str(),
            "print_int"
                | "print_string"
                | "print_char"
                | "print_float"
                | "print_double"
                | "fileRead"
                | "fileWrite"
                | "fileAppend"
                | "fileExists"
                | "workerSpawn"
                | "workerPoolSpawn"
                | "workerPost"
                | "workerRecv"
                | "workerPoolDispatch"
                | "workerTerminate"
        ) && !host.starts_with("gpu")
        {
            let zeros: Vec<String> = (0..imp.params.len())
                .map(|i| format!("(void)a{i};"))
                .collect();
            let body = if ret == "void" {
                zeros.join(" ")
            } else {
                format!("{} return 0;", zeros.join(" "))
            };
            out.push_str(&format!(
                "__attribute__((weak)) {ret} {host}({args}) {{ {body} }}\n"
            ));
        }
    }
    out.push_str("void print_int(int32_t v);\n");
    out.push_str("void print_string(dream_ptr s);\n");
    out.push_str("void print_char(int32_t c);\n");
    out.push_str("void print_float(float v);\n");
    out.push_str("void print_double(double v);\n\n");
}

fn emit_ftable_decl(out: &mut String, cx: &Cx<'_>, async_n: usize) {
    let n = cx.mir.functions.len() + 1 + async_n;
    out.push_str(&format!("static void *dream_ft[{n}];\n\n"));
}

fn emit_ftable_def(out: &mut String, cx: &Cx<'_>, _async_n: usize) {
    out.push_str("static void dream_init_ft(void) {\n");
    for f in &cx.mir.functions {
        let i = cx.func_index(f);
        let name = c_ident(&func_symbol(f));
        out.push_str(&format!("  dream_ft[{i}] = (void *){name};\n"));
    }
    let mut async_i = 0usize;
    for f in &cx.mir.functions {
        if !f.is_async {
            continue;
        }
        let i = cx.mir.functions.len() + 1 + async_i;
        out.push_str(&format!("  dream_ft[{i}] = (void *){};\n", poll_name(f)));
        async_i += 1;
    }
    out.push_str("}\n\n");
}

fn poll_name(f: &MirFunction) -> String {
    format!("poll_{}", c_ident(&func_symbol(f)))
}

fn emit_async_pair(out: &mut String, cx: &Cx<'_>, stub: &MirFunction, poll_idx: i32) {
    let Some(hir) = stub.hir_fn.as_ref() else {
        emit_func(out, cx, stub);
        return;
    };
    let mut body = crate::lower::lower_async_poll_body(hir, cx.interner);
    let _ = crate::passes::RcInsertion.run(&mut body, cx.interner);
    let mut size = 64i32;
    let mut offs = Vec::new();
    for decl in body.locals.iter() {
        if matches!(cx.interner.kind(decl.ty), TyKind::Void) {
            offs.push(0);
            continue;
        }
        let sz = crate::backend::c::types::native_scalar_size(cx, decl.ty)
            .0
            .max(8) as i32;
        size = (size + 7) & !7;
        offs.push(size);
        size += sz;
    }
    out.push_str(&format!("{} {{\n", proto(cx, stub)));
    out.push_str(&format!(
        "  dream_ptr __self = dream_new_future({size}, {poll_idx}, 0);\n"
    ));
    for (pi, p) in body.params.iter().enumerate() {
        let off = offs[p.0 as usize];
        let param_ty = body.local_ty(*p);
        if cx.interner.is_value_type(param_ty) {
            let size = crate::backend::c::types::native_scalar_size(cx, param_ty).0;
            out.push_str(&format!(
                "  memcpy((char *)dream_p(__self) + {off}, dream_p(l{}), {size});\n",
                p.0
            ));
        } else {
            let ty = c_ty(cx.interner, param_ty);
            out.push_str(&format!(
                "  *({ty} *)((char *)dream_p(__self) + {off}) = l{};\n",
                p.0
            ));
        }
        let _ = pi;
    }
    out.push_str("  dream_enqueue(__self);\n  return __self;\n}\n\n");

    out.push_str(&format!(
        "int32_t {}(dream_ptr __self) {{\n",
        poll_name(stub)
    ));
    for (i, decl) in body.locals.iter().enumerate() {
        if matches!(cx.interner.kind(decl.ty), TyKind::Void) {
            continue;
        }
        if cx.interner.is_value_type(decl.ty) {
            out.push_str(&format!(
                "  dream_ptr l{i} = (dream_ptr)((char *)dream_p(__self) + {});\n",
                offs[i]
            ));
        } else {
            out.push_str(&format!(
                "  {} l{i} = *({} *)((char *)dream_p(__self) + {});\n",
                local_c_ty(cx.interner, decl.ty),
                local_c_ty(cx.interner, decl.ty),
                offs[i]
            ));
        }
    }
    out.push_str("  int32_t __st = *(int32_t *)dream_p(__self);\n");
    out.push_str("  switch (__st) {\n");
    for (bi, _) in body.blocks.iter().enumerate() {
        if bi == body.entry.0 as usize {
            continue;
        }
        out.push_str(&format!("    case {bi}: goto L{bi};\n"));
    }
    out.push_str("    default: break;\n  }\n");
    out.push_str(&format!("  goto L{};\n", body.entry.0));
    let mut resume_dest: Vec<Option<u32>> = vec![None; body.blocks.len()];
    for block in &body.blocks {
        if let crate::Terminator::Await {
            dest: Some(d),
            resume,
            ..
        } = &block.terminator
        {
            if (resume.0 as usize) < resume_dest.len() {
                resume_dest[resume.0 as usize] = Some(d.0);
            }
        }
    }
    for (bi, block) in body.blocks.iter().enumerate() {
        out.push_str(&format!("L{bi}:;\n"));
        if let Some(d) = resume_dest[bi] {
            let dest_ty = body.local_ty(crate::Local(d));
            if cx.interner.is_value_type(dest_ty) {
                let size = crate::backend::c::types::native_scalar_size(cx, dest_ty).0;
                out.push_str(&format!(
                    "  {{\n    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);\n    memcpy(dream_p(l{d}), dream_p(*(dream_ptr *)((char *)dream_p(__ch) + 8)), {size});\n  }}\n"
                ));
            } else {
                let ty = local_c_ty(cx.interner, dest_ty);
                let value = match cx.interner.kind(dest_ty) {
                    TyKind::Prim(dream_types::PrimTy::Long | dream_types::PrimTy::ULong) => {
                        "*(int64_t *)((char *)dream_p(__ch) + 56)"
                    }
                    TyKind::Prim(dream_types::PrimTy::Float) => {
                        "*(float *)((char *)dream_p(__ch) + 56)"
                    }
                    TyKind::Prim(dream_types::PrimTy::Double) => {
                        "*(double *)((char *)dream_p(__ch) + 56)"
                    }
                    _ => "*(dream_ptr *)((char *)dream_p(__ch) + 8)",
                };
                out.push_str(&format!(
                    "  {{\n    dream_ptr __ch = *(dream_ptr *)((char *)dream_p(__self) + 36);\n    l{d} = ({ty}){value};\n  }}\n"
                ));
            }
        }
        for stmt in &block.stmts {
            emit_stmt(out, cx, &body, stmt);
        }
        for (i, decl) in body.locals.iter().enumerate() {
            if matches!(cx.interner.kind(decl.ty), TyKind::Void)
                || cx.interner.is_value_type(decl.ty)
            {
                continue;
            }
            out.push_str(&format!(
                "  *({} *)((char *)dream_p(__self) + {}) = l{i};\n",
                local_c_ty(cx.interner, decl.ty),
                offs[i]
            ));
        }
        emit_term(out, cx, &body, &block.terminator);
    }
    out.push_str("  return 0;\n}\n\n");
}

fn proto(cx: &Cx<'_>, f: &MirFunction) -> String {
    let name = c_ident(&func_symbol(f));
    let ret = if f.is_async {
        "dream_ptr"
    } else {
        c_ty(cx.interner, f.ret)
    };
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{} l{}", c_ty(cx.interner, f.local_ty(*p)), p.0))
        .collect();
    let args = if params.is_empty() {
        "void".into()
    } else {
        params.join(", ")
    };
    format!("{ret} {name}({args})")
}

fn emit_func(out: &mut String, cx: &Cx<'_>, f: &MirFunction) {
    out.push_str(&format!("{} {{\n", proto(cx, f)));
    let returns_value = cx.interner.is_value_type(f.ret);
    for p in &f.params {
        let ty = f.local_ty(*p);
        let decl = &f.locals[p.0 as usize];
        if !cx.interner.is_value_type(ty) || decl.is_ref || decl.name.as_deref() == Some("this") {
            continue;
        }
        let size = crate::backend::c::types::native_scalar_size(cx, ty)
            .0
            .max(1);
        out.push_str(&format!(
            "  _Alignas(8) unsigned char __vsp{}[{size}];\n  memcpy(__vsp{}, dream_p(l{}), {size});\n  l{} = (dream_ptr)(uintptr_t)__vsp{};\n",
            p.0, p.0, p.0, p.0, p.0
        ));
    }
    for (i, decl) in f.locals.iter().enumerate() {
        if f.params.iter().any(|p| p.0 == i as u32) {
            continue;
        }
        if matches!(cx.interner.kind(decl.ty), TyKind::Void) {
            continue;
        }
        if cx.interner.is_value_type(decl.ty) {
            let size = crate::backend::c::types::native_scalar_size(cx, decl.ty)
                .0
                .max(1);
            if returns_value {
                out.push_str(&format!(
                    "  dream_ptr l{i} = dream_malloc({size}, {});\n",
                    crate::abi::TAG_STRUCT_BASE
                ));
            } else {
                out.push_str(&format!(
                    "  _Alignas(8) unsigned char __vs{i}[{size}];\n  dream_ptr l{i} = (dream_ptr)(uintptr_t)__vs{i};\n"
                ));
            }
        } else {
            out.push_str(&format!(
                "  {} l{i} = 0;\n",
                local_c_ty(cx.interner, decl.ty)
            ));
        }
    }
    out.push_str(&format!("  goto L{};\n", f.entry.0));
    for (bi, block) in f.blocks.iter().enumerate() {
        out.push_str(&format!("L{bi}:;\n"));
        for stmt in &block.stmts {
            emit_stmt(out, cx, f, stmt);
        }
        emit_term(out, cx, f, &block.terminator);
    }
    out.push_str("}\n\n");
}

/// `.c` files the native linker must compile with generated user C.
pub fn native_runtime_c_files() -> Vec<std::path::PathBuf> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime/c/native");
    [
        "heap.c",
        "strings.c",
        "object.c",
        "format.c",
        "panic.c",
        "weak.c",
        "closure.c",
        "async.c",
        "sync.c",
        "pike.c",
        "simd.c",
        "host.c",
        "worker.c",
    ]
    .iter()
    .map(|n| root.join(n))
    .collect()
}

pub fn native_runtime_include_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/runtime/c/native/include")
}
