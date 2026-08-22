use super::ast::{CTy, Expr, Item, Param, Stmt};
use super::builder::{FuncBuilder, ModuleBuilder};
use super::ctx::Cx;
use super::emit::Emitter;
use super::protocol::{emit_iface_init, emit_iface_trampolines, emit_protocol};
use super::release::emit_release_helpers;
use super::target::CTarget;
use super::types::{c_ident, c_ty, fn_ptr_abi, local_c_ty};
use crate::backend::shared::func_symbol;
use crate::{Mir, MirFunction, Rvalue, Statement};
use dream_abi::js_abi;
use dream_types::{TyKind, TypeId, TypeInterner};
use indexmap::IndexMap;

const NATIVE_RT_INCLUDE: &str = "\"dream_rt_native.h\"";
const WASM32_RT_INCLUDE: &str = "\"dream_rt_wasm32.h\"";

pub fn emit_c_module(mir: &Mir, interner: &TypeInterner) -> String {
    emit_c_module_for(mir, interner, CTarget::Native)
}

pub fn emit_c_module_for(mir: &Mir, interner: &TypeInterner, target: CTarget) -> String {
    let cx = Cx::new(mir, interner, target);
    let mut m = ModuleBuilder::new();
    m.include(if target.is_wasm32() {
        WASM32_RT_INCLUDE
    } else {
        NATIVE_RT_INCLUDE
    });
    if !target.is_wasm32() {
        m.include("<math.h>");
        m.include("<stdio.h>");
        m.include("<stdlib.h>");
    }
    m.include("<stdint.h>");
    m.include("<string.h>");
    emit_fn_typedefs(&mut m, &cx);
    emit_string_table(&mut m, &cx);
    emit_globals(&mut m, &cx);
    let async_n = mir.functions.iter().filter(|f| f.is_async).count();
    let import_async_n = mir
        .imports
        .iter()
        .filter(|i| lazy_import_poll(&cx, i).is_some())
        .count();
    // Poll table layout: [0] reserved, [1..functions] sync/async stubs,
    // [functions+1 .. +async_n] coroutine polls, then deferred-extern polls.
    let import_poll_base = mir.functions.len() + 1 + async_n;
    emit_imports(&mut m, &cx, import_poll_base);
    super::js_marshal::emit_js_marshal(&mut m, &cx);
    emit_ftable_decl(&mut m, &cx, async_n + import_async_n);
    for f in &mir.functions {
        let (ret, name, params, attr) = proto_parts(&cx, f);
        m.push(Item::Proto {
            static_: false,
            ret,
            name,
            params,
            import: None,
            export: None,
        });
        let _ = attr;
        if f.is_async {
            m.proto(
                CTy::I32,
                poll_name(f),
                vec![Param {
                    ty: CTy::Ptr,
                    name: "__self".into(),
                }],
            );
        }
    }
    for imp in &mir.imports {
        if let Some(poll_sym) = lazy_import_poll(&cx, imp) {
            m.proto(
                CTy::I32,
                poll_sym,
                vec![Param {
                    ty: CTy::Ptr,
                    name: "__self".into(),
                }],
            );
        }
    }
    emit_release_helpers(&mut m, &cx);
    emit_protocol(&mut m, &cx);
    emit_iface_trampolines(&mut m, &cx);
    let mut async_i = 0usize;
    for f in &mir.functions {
        if f.is_async {
            let poll_idx = mir.functions.len() + 1 + async_i;
            emit_async_pair(&mut m, &cx, f, poll_idx as i32);
            async_i += 1;
        } else {
            emit_func(&mut m, &cx, f);
        }
    }
    emit_ftable_def(&mut m, &cx, async_n + import_async_n);
    let mut ft_get = FuncBuilder::new(CTy::VoidPtr, "dream_ft_get");
    if cx.target.is_wasm32() {
        ft_get.export = Some(crate::abi::EXPORT_FT_GET.to_string());
    }
    ft_get.param(CTy::I32, "i");
    ft_get.stmt(Stmt::Return(Some(Expr::ternary(
        Expr::and(
            Expr::bin(crate::BinOp::Gt, Expr::id("i"), Expr::i(0)),
            Expr::lt(
                Expr::id("i"),
                Expr::i((mir.functions.len() + 1 + async_n + import_async_n) as i64),
            ),
        ),
        Expr::index(Expr::id("dream_ft"), Expr::id("i")),
        Expr::i(0),
    ))));
    m.push_func(ft_get);
    emit_iface_init(&mut m, &cx);
    emit_runtime_init(&mut m, &cx);
    emit_worker_invoke(&mut m, &cx);
    if let Some(main) = mir
        .functions
        .iter()
        .find(|f| f.name == crate::abi::ENTRY_FN)
    {
        emit_guest_entry(&mut m, &cx, main, async_n);
    }
    m.finish()
}

fn emit_guest_entry(m: &mut ModuleBuilder, cx: &Cx<'_>, main: &MirFunction, async_n: usize) {
    let mut entry = FuncBuilder::new(CTy::I32, crate::abi::GUEST_ENTRY_FN);
    if cx.target.is_wasm32() {
        entry.export = Some(crate::abi::ENTRY_FN.to_string());
    }
    entry.call("dream_runtime_init", vec![]);
    let main_args = if main.params.is_empty() {
        vec![]
    } else {
        vec![Expr::call("dream_array_new", vec![Expr::i(0), Expr::i(8)])]
    };
    if main.is_async {
        entry.stmt(Stmt::decl(
            CTy::Ptr,
            "__mf",
            Some(Expr::call("main_dream", main_args)),
        ));
        // Futures are lazy; the entry point launches async main explicitly.
        entry.call("dream_start", vec![Expr::id("__mf")]);
    } else {
        entry.call("main_dream", main_args);
    }
    if async_n > 0 {
        entry.call("dream_run_loop", vec![]);
    }
    if cx.mir.uses_defer {
        entry.call("dream_defer_drain_all", vec![]);
    }
    if cx.target.is_wasm32() && main.is_async {
        entry.ret(Some(Expr::cast(CTy::I32, Expr::id("__mf"))));
    } else {
        entry.ret(Some(Expr::i(0)));
    }
    m.push_func(entry);
    if cx.target.is_wasm32() {
        return;
    }
    let mut main_fn = FuncBuilder::new(CTy::I32, "main");
    main_fn.param(CTy::I32, "argc");
    main_fn.param(CTy::ptr_to(CTy::CharPtr), "argv");
    main_fn.call(
        "dream_process_capture_args",
        vec![Expr::id("argc"), Expr::id("argv")],
    );
    main_fn.ret(Some(Expr::call(crate::abi::GUEST_ENTRY_FN, vec![])));
    m.push_func(main_fn);
}

fn emit_string_table(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let header = if cx.target.is_wasm32() {
        crate::abi::HEAP_HEADER_SIZE as i64
    } else {
        crate::abi::NATIVE_HEAP_HEADER_SIZE as i64
    };
    for (s, sym) in &cx.strings {
        let units: Vec<u16> = s.encode_utf16().collect();
        let n = units.len();
        let init_units: Vec<Expr> = if units.is_empty() {
            vec![Expr::i(0)]
        } else {
            units.iter().map(|u| Expr::i(*u as i64)).collect()
        };
        let (fields, init) = if cx.target.is_wasm32() {
            (
                vec![
                    (CTy::I32, "size".into()),
                    (CTy::I32, "tag".into()),
                    (CTy::I32, "rc".into()),
                    (CTy::I32, "len".into()),
                    (CTy::I32, "pad".into()),
                    (
                        CTy::Array {
                            elem: Box::new(CTy::U16),
                            len: n.max(1),
                        },
                        "u".into(),
                    ),
                ],
                vec![
                    Expr::i(0),
                    Expr::id("TAG_STRING"),
                    Expr::id("INT32_MAX"),
                    Expr::i(n as i64),
                    Expr::i(0),
                    Expr::Compound(init_units),
                ],
            )
        } else {
            (
                vec![
                    (CTy::I32, "size".into()),
                    (CTy::I32, "header_pad".into()),
                    (CTy::I32, "tag".into()),
                    (CTy::I32, "rc".into()),
                    (CTy::I32, "len".into()),
                    (CTy::I32, "pad".into()),
                    (
                        CTy::Array {
                            elem: Box::new(CTy::U16),
                            len: n.max(1),
                        },
                        "u".into(),
                    ),
                ],
                vec![
                    Expr::i(0),
                    Expr::i(0),
                    Expr::id("TAG_STRING"),
                    Expr::id("INT32_MAX"),
                    Expr::i(n as i64),
                    Expr::i(0),
                    Expr::Compound(init_units),
                ],
            )
        };
        m.push(Item::Global {
            thread_local: false,
            align: None,
            static_: true,
            const_: false,
            ty: CTy::Struct { fields },
            name: format!("{sym}_blk"),
            init: Some(Expr::Compound(init)),
        });
        m.push(Item::Global {
            thread_local: false,
            align: None,
            static_: true,
            const_: true,
            ty: CTy::Ptr,
            name: sym.clone(),
            init: Some(Expr::cast(
                CTy::Ptr,
                Expr::add(
                    Expr::cast(CTy::CharPtr, Expr::addr_of(Expr::id(format!("{sym}_blk")))),
                    Expr::i(header),
                ),
            )),
        });
    }
}

fn emit_globals(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    for g in &cx.mir.globals {
        if g.id.0 == 0 {
            // Native TLS `g0`. Wasm32 uses a per-instance wasm global (`dream_g0_get`/`set`)
            // because `_Thread_local` is linear-memory TLS that aliases the stack under
            // `--shared-memory`.
            if !cx.target.is_wasm32() {
                m.push(Item::Global {
                    thread_local: true,
                    align: None,
                    static_: false,
                    const_: false,
                    ty: CTy::Ptr,
                    name: "g0".into(),
                    init: Some(Expr::i(0)),
                });
            }
            continue;
        }
        if cx.interner.is_value_type(g.ty) {
            let size = super::types::elem_size(cx, g.ty).max(1) as usize;
            m.push(Item::Global {
                thread_local: false,
                align: Some(8),
                static_: true,
                const_: false,
                ty: CTy::Array {
                    elem: Box::new(CTy::Named("unsigned char")),
                    len: size,
                },
                name: format!("__vg{}", g.id.0),
                init: None,
            });
            m.push(Item::Global {
                thread_local: false,
                align: None,
                static_: false,
                const_: false,
                ty: CTy::Ptr,
                name: format!("g{}", g.id.0),
                init: Some(Expr::cast(
                    CTy::Ptr,
                    Expr::cast(CTy::Named("uintptr_t"), Expr::id(format!("__vg{}", g.id.0))),
                )),
            });
            continue;
        }
        m.push(Item::Global {
            thread_local: false,
            align: None,
            static_: false,
            const_: false,
            ty: c_ty(cx.interner, g.ty),
            name: format!("g{}", g.id.0),
            init: Some(Expr::i(0)),
        });
    }
}

fn emit_worker_invoke(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let has_env = cx.mir.globals.iter().any(|g| g.id.0 == 0);
    let result_off = cx.target.abi().future.result as i64;
    let mut raw = FuncBuilder::new(CTy::Ptr, "dream_worker_invoke_raw");
    if cx.target.is_wasm32() {
        raw.export = Some(crate::abi::EXPORT_WORKER_INVOKE_RAW.to_string());
    }
    raw.param(CTy::I32, "fn");
    raw.param(CTy::Ptr, "env");
    raw.param(CTy::Ptr, "arg");
    if cx.target.is_wasm32() {
        raw.call("dream_runtime_init", vec![]);
    }
    raw.stmt(Stmt::if_(
        Expr::bin(crate::BinOp::Le, Expr::id("fn"), Expr::i(0)),
        Stmt::Return(Some(Expr::i(0))),
    ));
    if has_env {
        raw.assign(Expr::id("g0"), Expr::id("env"));
    } else {
        raw.expr_stmt(Expr::cast(CTy::Void, Expr::id("env")));
    }
    raw.stmt(Stmt::decl(
        CTy::Ptr,
        "__r",
        Some(Expr::IndirectCall {
            callee: Box::new(Expr::cast(
                CTy::Ident("dream_fn_ptr__ptr".into()),
                Expr::index(Expr::id("dream_ft"), Expr::id("fn")),
            )),
            args: vec![Expr::id("arg")],
        }),
    ));
    if cx.target.is_wasm32() {
        // Worker bodies cross the string wire, so a tag-0 return can only be a lazy Future frame;
        // launch it so the JS-side F_STATUS polling observes progress. Native returns can be
        // untagged scalars, so the native path starts futures in `dream_worker_invoke` instead,
        // where the async fn indices are known exactly.
        raw.stmt(Stmt::if_(
            Expr::bin(
                crate::BinOp::Eq,
                Expr::call("dream_object_tag", vec![Expr::id("__r")]),
                Expr::i(0),
            ),
            Stmt::call("dream_start", vec![Expr::id("__r")]),
        ));
    }
    raw.stmt(Stmt::Return(Some(Expr::id("__r"))));
    m.push_func(raw);

    let mut b = FuncBuilder::new(CTy::Ptr, "dream_worker_invoke");
    if cx.target.is_wasm32() {
        b.export = Some(crate::abi::EXPORT_WORKER_INVOKE.to_string());
    }
    b.param(CTy::I32, "fn");
    b.param(CTy::Ptr, "env");
    b.param(CTy::Ptr, "arg");
    b.stmt(Stmt::decl(
        CTy::Ptr,
        "result",
        Some(Expr::call(
            "dream_worker_invoke_raw",
            vec![Expr::id("fn"), Expr::id("env"), Expr::id("arg")],
        )),
    ));
    let async_indices: Vec<_> = cx
        .mir
        .functions
        .iter()
        .filter(|f| f.is_async)
        .map(|f| cx.func_index(f))
        .collect();
    if !async_indices.is_empty() {
        let mut arms = Vec::new();
        for index in async_indices {
        arms.push(super::ast::SwitchArm {
            keys: vec![super::ast::CaseKey::Int(index as i64)],
            body: vec![
                // The constructor returned a lazy future; launch it before draining.
                Stmt::call("dream_start", vec![Expr::id("result")]),
                Stmt::call("dream_run_loop", vec![]),
                Stmt::Return(Some(Expr::load(
                    CTy::Ptr,
                    Expr::ptr_add(Expr::id("result"), Expr::i(result_off)),
                ))),
            ],
        });
        }
        arms.push(super::ast::SwitchArm {
            keys: vec![],
            body: vec![Stmt::Expr(Expr::id("break"))],
        });
        b.stmt(Stmt::Switch {
            expr: Expr::id("fn"),
            arms,
        });
    }
    b.ret(Some(Expr::id("result")));
    m.push_func(b);
}

fn emit_imports(m: &mut ModuleBuilder, cx: &Cx<'_>, import_poll_base: usize) {
    if cx.target.is_wasm32() {
        emit_wasm32_js_rc(m);
    }
    let fut = cx.target.abi().future;
    let mut poll_i = 0usize;
    for imp in &cx.mir.imports {
        if super::c_imports::is_c_import(imp) {
            if cx.target.is_wasm32() {
                continue;
            }
            super::c_imports::emit_c_import(m, cx, imp);
            continue;
        }
        if cx.target.is_wasm32()
            && (imp.field == js_abi::HOST_JS_RETAIN || imp.field == js_abi::HOST_JS_RELEASE)
        {
            continue;
        }
        let host = super::types::import_host_name(imp);
        let async_wrap = super::types::import_is_async_future(cx.mir, imp);
        if !cx.target.is_wasm32() && !async_wrap && super::types::native_header_declares(&host) {
            continue;
        }
        let name = super::types::import_call_name(cx.mir, imp);
        let host_ret = if async_wrap {
            match cx.interner.kind(imp.ret.unwrap()) {
                TyKind::Struct(_, args) => match args.first() {
                    Some(t) if matches!(cx.interner.kind(*t), TyKind::Void) => CTy::I32,
                    Some(t) => c_ty(cx.interner, *t),
                    None => CTy::I32,
                },
                _ => CTy::I32,
            }
        } else {
            imp.ret.map(|t| c_ty(cx.interner, t)).unwrap_or(CTy::Void)
        };
        let params: Vec<Param> = imp
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| Param {
                ty: if imp.param_by_ref.get(i).copied().unwrap_or(false) {
                    CTy::Ptr
                } else {
                    c_ty(cx.interner, *t)
                },
                name: format!("a{i}"),
            })
            .collect();
        let import_mod = if imp.module.is_empty() {
            js_abi::HOST_MODULE.to_string()
        } else {
            imp.module.clone()
        };
        let import_field = if imp.field.is_empty() {
            imp.name.clone()
        } else {
            imp.field.clone()
        };
        // Async host bridges are lazy like every other future: calling the extern constructs an
        // unstarted Future carrying the marshaled args; the first poll performs the host call.
        // On wasm32 the JS wrapper settles the guest-provided future via `__dream_resolve`, so
        // the import takes the future as its leading argument. Native hosts are blocking calls
        // that keep their declared signatures; their poll completes the future directly.
        if async_wrap {
            let slot_base = fut.slots as i64;
            let frame_size = slot_base + (params.len() as i64) * 8;
            let poll_sym = format!("{name}_poll");
            if cx.target.is_wasm32() {
                let mut proto_params = vec![Param {
                    ty: CTy::Ptr,
                    name: "__f".into(),
                }];
                proto_params.extend(params.iter().cloned());
                m.import_proto(CTy::Void, host.clone(), proto_params, import_mod, import_field);
            } else if !super::types::native_header_declares(&host) {
                m.proto(host_ret.clone(), host.clone(), params.clone());
            }

            let poll_idx = (import_poll_base + poll_i) as i64;
            poll_i += 1;

            let mut b = FuncBuilder::new(CTy::Ptr, name);
            b.params = params.clone();
            b.stmt(Stmt::decl(
                CTy::Ptr,
                "__self",
                Some(Expr::call(
                    "dream_new_future",
                    vec![
                        Expr::i(frame_size),
                        Expr::i(poll_idx),
                        Expr::i(crate::abi::FUTURE_KIND_TASK as i64),
                    ],
                )),
            ));
            for (i, p) in params.iter().enumerate() {
                b.stmt(Stmt::store(
                    p.ty.clone(),
                    Expr::ptr_add(Expr::id("__self"), Expr::i(slot_base + (i as i64) * 8)),
                    Expr::id(p.name.clone()),
                ));
            }
            b.ret(Some(Expr::id("__self")));
            m.push_func(b);

            let mut poll = FuncBuilder::new(CTy::I32, poll_sym);
            poll.param(CTy::Ptr, "__self");
            poll.stmt(Stmt::decl(
                CTy::I32,
                "__st",
                Some(Expr::load(
                    CTy::I32,
                    Expr::ptr_add(Expr::id("__self"), Expr::i(fut.state as i64)),
                )),
            ));
            poll.stmt(Stmt::if_(
                Expr::bin(crate::BinOp::Ne, Expr::id("__st"), Expr::i(0)),
                Stmt::Goto("Done".into()),
            ));
            let saved_args: Vec<Expr> = (0..params.len())
                .map(|i| {
                    Expr::load(
                        params[i].ty.clone(),
                        Expr::ptr_add(Expr::id("__self"), Expr::i(slot_base + (i as i64) * 8)),
                    )
                })
                .collect();
            if cx.target.is_wasm32() {
                poll.call(
                    host.clone(),
                    std::iter::once(Expr::id("__self"))
                        .chain(saved_args)
                        .collect(),
                );
            } else if host_ret == CTy::Void {
                poll.call(host.clone(), saved_args);
                poll.call(
                    "dream_async_complete",
                    vec![Expr::id("__self"), Expr::i(0)],
                );
            } else {
                let host_call = Expr::call(host.clone(), saved_args);
                let as_ptr =
                    Expr::cast(CTy::Ptr, Expr::cast(CTy::Named("intptr_t"), host_call));
                poll.call("dream_async_complete", vec![Expr::id("__self"), as_ptr]);
            }
            poll.stmt(Stmt::store(
                CTy::I32,
                Expr::ptr_add(Expr::id("__self"), Expr::i(fut.state as i64)),
                Expr::i(1),
            ));
            poll.label("Done");
            poll.ret(Some(Expr::i(0)));
            m.push_func(poll);
            continue;
        }
        if cx.target.is_wasm32() {
            m.import_proto(
                host_ret.clone(),
                host.clone(),
                params.clone(),
                import_mod,
                import_field,
            );
        } else if !super::types::native_header_declares(&host) {
            m.proto(host_ret.clone(), host.clone(), params.clone());
        }
    }
}

fn emit_wasm32_js_rc(m: &mut ModuleBuilder) {
    let handle = vec![Param {
        ty: CTy::Ptr,
        name: "h".into(),
    }];
    m.import_proto(
        CTy::Void,
        "js_retain",
        handle.clone(),
        js_abi::HOST_MODULE,
        js_abi::HOST_JS_RETAIN,
    );
    m.import_proto(
        CTy::Void,
        "js_release",
        handle,
        js_abi::HOST_MODULE,
        js_abi::HOST_JS_RELEASE,
    );
}

fn emit_ftable_decl(m: &mut ModuleBuilder, cx: &Cx<'_>, async_n: usize) {
    let n = cx.mir.functions.len() + 1 + async_n;
    m.push(Item::Global {
        // Linker table indices are the same in every instance of this module; sharing BSS is
        // required so a worker instance can call a funcref posted from the parent.
        thread_local: false,
        align: None,
        static_: true,
        const_: false,
        ty: CTy::Array {
            elem: Box::new(CTy::VoidPtr),
            len: n,
        },
        name: "dream_ft".into(),
        init: None,
    });
}

fn emit_ftable_def(m: &mut ModuleBuilder, cx: &Cx<'_>, _ft_extra: usize) {
    let mut b = FuncBuilder::new(CTy::Void, "dream_init_ft");
    b.static_ = true;
    let coroutine_n = cx.mir.functions.iter().filter(|f| f.is_async).count();
    for f in &cx.mir.functions {
        let i = cx.func_index(f);
        let name = c_ident(&func_symbol(f));
        b.assign(
            Expr::index(Expr::id("dream_ft"), Expr::i(i as i64)),
            Expr::cast(CTy::VoidPtr, Expr::id(name)),
        );
    }
    let mut async_i = 0usize;
    for f in &cx.mir.functions {
        if !f.is_async {
            continue;
        }
        let i = cx.mir.functions.len() + 1 + async_i;
        b.assign(
            Expr::index(Expr::id("dream_ft"), Expr::i(i as i64)),
            Expr::cast(CTy::VoidPtr, Expr::id(poll_name(f))),
        );
        async_i += 1;
    }
    let mut import_i = 0usize;
    for imp in &cx.mir.imports {
        if let Some(poll_sym) = lazy_import_poll(cx, imp) {
            let i = cx.mir.functions.len() + 1 + coroutine_n + import_i;
            b.assign(
                Expr::index(Expr::id("dream_ft"), Expr::i(i as i64)),
                Expr::cast(CTy::VoidPtr, Expr::id(poll_sym)),
            );
            import_i += 1;
        }
    }
    m.push_func(b);
}

/// A deferred-extern poll symbol for `imp`, or `None` when the import is not an async host bridge
/// (C FFI imports keep their synchronous wrappers).
fn lazy_import_poll(cx: &Cx<'_>, imp: &dream_hir::HImport) -> Option<String> {
    if super::c_imports::is_c_import(imp) || !super::types::import_is_async_future(cx.mir, imp) {
        return None;
    }
    Some(format!(
        "{}_poll",
        super::types::import_call_name(cx.mir, imp)
    ))
}

fn emit_runtime_init(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    m.push(Item::Global {
        thread_local: false,
        align: None,
        static_: true,
        const_: false,
        ty: CTy::I32,
        name: "dream_rt_inited".into(),
        init: Some(Expr::i(0)),
    });
    let mut b = FuncBuilder::new(CTy::Void, "dream_runtime_init");
    if cx.target.is_wasm32() {
        b.export = Some(crate::abi::EXPORT_RUNTIME_INIT.to_string());
    }
    b.stmt(Stmt::if_(
        Expr::id("dream_rt_inited"),
        Stmt::Return(None),
    ));
    b.assign(Expr::id("dream_rt_inited"), Expr::i(1));
    if cx.target.is_wasm32() {
        b.call("dream_heap_init", vec![]);
    }
    b.call("dream_init_ft", vec![]);
    b.call("dream_init_itables", vec![]);
    if !cx.target.is_wasm32() {
        b.call(
            "dream_host_bind",
            vec![Expr::id("dream_string_alloc"), Expr::id("dream_array_new")],
        );
    }
    if let Some(init) = cx
        .mir
        .functions
        .iter()
        .find(|f| f.name == crate::lower::INIT_FN_NAME)
    {
        b.call(c_ident(&func_symbol(init)), vec![]);
    }
    m.push_func(b);
}

fn poll_name(f: &MirFunction) -> String {
    format!("poll_{}", c_ident(&func_symbol(f)))
}

fn proto_parts(cx: &Cx<'_>, f: &MirFunction) -> (CTy, String, Vec<Param>, Option<&'static str>) {
    let name = c_ident(&func_symbol(f));
    let attr = if name.to_ascii_lowercase().contains("sink") {
        Some("__attribute__((noinline, noclone))")
    } else {
        None
    };
    let ret = if f.is_async {
        CTy::Ptr
    } else {
        c_ty(cx.interner, f.ret)
    };
    let params: Vec<Param> = f
        .params
        .iter()
        .map(|p| Param {
            ty: c_ty(cx.interner, f.local_ty(*p)),
            name: format!("l{}", p.0),
        })
        .collect();
    (ret, name, params, attr)
}

fn emit_async_pair(m: &mut ModuleBuilder, cx: &Cx<'_>, stub: &MirFunction, poll_idx: i32) {
    let Some(hir) = stub.hir_fn.as_ref() else {
        emit_func(m, cx, stub);
        return;
    };
    let mut body = crate::lower::lower_async_poll_body(hir, cx.interner);
    let _ = crate::passes::RcInsertion::run_with_layouts(&mut body, cx.interner, &cx.mir.layouts);
    let fut = cx.target.abi().future;
    let slots = crate::async_emit::layout_async_slots(&body, cx.interner, fut.slots as i32, |ty| {
        let sz = super::types::native_scalar_size(cx, ty).0.max(8);
        (sz, cx.interner.is_value_type(ty))
    });
    let size = slots.frame_size;
    let offs: Vec<i32> = (0..body.locals.len())
        .map(|i| slots.offsets.get(&i).copied().unwrap_or(0))
        .collect();
    let (ret, name, params, attr) = proto_parts(cx, stub);
    let mut stub_fn = FuncBuilder::new(ret, name);
    stub_fn.attr = attr;
    stub_fn.params = params;
    stub_fn.stmt(Stmt::decl(
        CTy::Ptr,
        "__self",
        Some(Expr::call(
            "dream_new_future",
            vec![Expr::i(size as i64), Expr::i(poll_idx as i64), Expr::i(0)],
        )),
    ));
    for p in body.params.iter() {
        let off = offs[p.0 as usize];
        let param_ty = body.local_ty(*p);
        if cx.interner.is_value_type(param_ty) {
            let sz = super::types::native_scalar_size(cx, param_ty).0;
            stub_fn.call(
                "memcpy",
                vec![
                    Expr::ptr_add(Expr::id("__self"), Expr::i(off as i64)),
                    Expr::dream_p(Expr::local(p.0)),
                    Expr::i(sz as i64),
                ],
            );
        } else {
            let ty = local_c_ty(cx, param_ty);
            stub_fn.stmt(Stmt::store(
                ty,
                Expr::ptr_add(Expr::id("__self"), Expr::i(off as i64)),
                Expr::local(p.0),
            ));
        }
    }
    stub_fn.ret(Some(Expr::id("__self")));
    m.push_func(stub_fn);

    let mut poll = FuncBuilder::new(CTy::I32, poll_name(stub));
    poll.param(CTy::Ptr, "__self");
    for (i, decl) in body.locals.iter().enumerate() {
        if matches!(cx.interner.kind(decl.ty), TyKind::Void) {
            continue;
        }
        if cx.interner.is_value_type(decl.ty) {
            poll.stmt(Stmt::decl(
                CTy::Ptr,
                format!("l{i}"),
                Some(Expr::cast(
                    CTy::Ptr,
                    Expr::ptr_add(Expr::id("__self"), Expr::i(offs[i] as i64)),
                )),
            ));
        } else {
            let ty = local_c_ty(cx, decl.ty);
            poll.stmt(Stmt::decl(
                ty.clone(),
                format!("l{i}"),
                Some(Expr::load(
                    ty,
                    Expr::ptr_add(Expr::id("__self"), Expr::i(offs[i] as i64)),
                )),
            ));
        }
    }
    poll.stmt(Stmt::decl(
        CTy::I32,
        "__st",
        Some(Expr::load(
            CTy::I32,
            Expr::ptr_add(
                Expr::id("__self"),
                Expr::i(cx.target.abi().future.state as i64),
            ),
        )),
    ));
    let mut arms = Vec::new();
    for (bi, _) in body.blocks.iter().enumerate() {
        if bi == body.entry.0 as usize {
            continue;
        }
        arms.push(super::ast::SwitchArm {
            keys: vec![super::ast::CaseKey::Int(bi as i64)],
            body: vec![Stmt::Goto(format!("L{bi}"))],
        });
    }
    arms.push(super::ast::SwitchArm {
        keys: vec![],
        body: vec![Stmt::Expr(Expr::id("break"))],
    });
    poll.stmt(Stmt::Switch {
        expr: Expr::id("__st"),
        arms,
    });
    poll.goto(format!("L{}", body.entry.0));
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
        poll.label(format!("L{bi}"));
        if let Some(d) = resume_dest[bi] {
            let dest_ty = body.local_ty(crate::Local(d));
            let fut = cx.target.abi().future;
            if cx.interner.is_value_type(dest_ty) {
                let sz = super::types::native_scalar_size(cx, dest_ty).0;
                poll.stmt(Stmt::block(vec![
                    Stmt::decl(
                        CTy::Ptr,
                        "__ch",
                        Some(Expr::load(
                            CTy::Ptr,
                            Expr::ptr_add(Expr::id("__self"), Expr::i(fut.awaiting as i64)),
                        )),
                    ),
                    Stmt::call(
                        "memcpy",
                        vec![
                            Expr::dream_p(Expr::local(d)),
                            Expr::dream_p(Expr::load(
                                CTy::Ptr,
                                Expr::ptr_add(Expr::id("__ch"), Expr::i(fut.result as i64)),
                            )),
                            Expr::i(sz as i64),
                        ],
                    ),
                ]));
            } else {
            let ty = local_c_ty(cx, dest_ty);
                let value = match cx.interner.kind(dest_ty) {
                    TyKind::Prim(dream_types::PrimTy::Long | dream_types::PrimTy::ULong) => {
                        Expr::load(
                            CTy::I64,
                            Expr::ptr_add(Expr::id("__ch"), Expr::i(fut.wide as i64)),
                        )
                    }
                    TyKind::Prim(dream_types::PrimTy::Float) => Expr::load(
                        CTy::F32,
                        Expr::ptr_add(Expr::id("__ch"), Expr::i(fut.wide as i64)),
                    ),
                    TyKind::Prim(dream_types::PrimTy::Double) => Expr::load(
                        CTy::F64,
                        Expr::ptr_add(Expr::id("__ch"), Expr::i(fut.wide as i64)),
                    ),
                    _ => Expr::load(
                        CTy::Ptr,
                        Expr::ptr_add(Expr::id("__ch"), Expr::i(fut.result as i64)),
                    ),
                };
                poll.stmt(Stmt::block(vec![
                    Stmt::decl(
                        CTy::Ptr,
                        "__ch",
                        Some(Expr::load(
                            CTy::Ptr,
                            Expr::ptr_add(Expr::id("__self"), Expr::i(fut.awaiting as i64)),
                        )),
                    ),
                    Stmt::assign(Expr::local(d), Expr::cast(ty, value)),
                ]));
            }
        }
        {
            let mut e = Emitter::new(cx, &body, &mut poll);
            e.stmts(&block.stmts);
        }
        // Spill back only the locals this block *modified*: every poll invocation reloads all
        // locals from the frame before the state dispatch, so unmodified locals still satisfy
        // the frame == register invariant. Storing everything here cost O(blocks × locals)
        // redundant heap stores per poll.
        let mut dirty: Vec<u32> = Vec::new();
        // The await-result handoff (`__ch` → local) emitted above this label is not a block
        // statement, so seed it explicitly.
        if let Some(d) = resume_dest[bi] {
            let decl = &body.locals[d as usize];
            if !cx.interner.is_value_type(decl.ty) {
                dirty.push(d);
            }
        }
        for s in &block.stmts {
            if let Statement::Assign(crate::Place::Local(l), _) = s {
                let i = l.0 as usize;
                let decl = &body.locals[i];
                if !matches!(cx.interner.kind(decl.ty), TyKind::Void)
                    && !cx.interner.is_value_type(decl.ty)
                    && !dirty.contains(&l.0)
                {
                    dirty.push(l.0);
                }
            }
        }
        for i in dirty {
            poll.stmt(Stmt::store(
                local_c_ty(cx, body.local_ty(crate::Local(i))),
                Expr::ptr_add(Expr::id("__self"), Expr::i(offs[i as usize] as i64)),
                Expr::local(i),
            ));
        }
        {
            let mut e = Emitter::new(cx, &body, &mut poll);
            e.term(&block.terminator);
        }
    }
    poll.ret(Some(Expr::i(0)));
    m.push_func(poll);
}

fn emit_func(m: &mut ModuleBuilder, cx: &Cx<'_>, f: &MirFunction) {
    let (ret, name, params, attr) = proto_parts(cx, f);
    let mut b = FuncBuilder::new(ret, name);
    b.attr = attr;
    b.params = params;
    for p in &f.params {
        let ty = f.local_ty(*p);
        let decl = &f.locals[p.0 as usize];
        if !cx.interner.is_value_type(ty) || decl.is_ref || decl.name.as_deref() == Some("this") {
            continue;
        }
        let size = super::types::native_scalar_size(cx, ty).0.max(1) as usize;
        b.stmt(Stmt::Decl {
            align: Some(8),
            static_: false,
            const_: false,
            ty: CTy::Array {
                elem: Box::new(CTy::Named("unsigned char")),
                len: size,
            },
            name: format!("__vsp{}", p.0),
            init: None,
        });
        b.call(
            "memcpy",
            vec![
                Expr::id(format!("__vsp{}", p.0)),
                Expr::dream_p(Expr::local(p.0)),
                Expr::i(size as i64),
            ],
        );
        b.assign(
            Expr::local(p.0),
            Expr::cast(
                CTy::Ptr,
                Expr::cast(CTy::Named("uintptr_t"), Expr::id(format!("__vsp{}", p.0))),
            ),
        );
    }
    for (i, decl) in f.locals.iter().enumerate() {
        if f.params.iter().any(|p| p.0 == i as u32) {
            continue;
        }
        if matches!(cx.interner.kind(decl.ty), TyKind::Void) {
            continue;
        }
        if cx.interner.is_value_type(decl.ty) {
            let size = super::types::native_scalar_size(cx, decl.ty).0.max(1) as usize;
            b.stmt(Stmt::Decl {
                align: Some(8),
                static_: false,
                const_: false,
                ty: CTy::Array {
                    elem: Box::new(CTy::Named("unsigned char")),
                    len: size,
                },
                name: format!("__vs{i}"),
                init: Some(Expr::Compound(vec![Expr::i(0)])),
            });
            b.stmt(Stmt::decl(
                CTy::Ptr,
                format!("l{i}"),
                Some(Expr::cast(
                    CTy::Ptr,
                    Expr::cast(CTy::Named("uintptr_t"), Expr::id(format!("__vs{i}"))),
                )),
            ));
        } else {
            b.stmt(Stmt::decl(
                local_c_ty(cx, decl.ty),
                format!("l{i}"),
                Some(Expr::i(0)),
            ));
        }
    }
    b.goto(format!("L{}", f.entry.0));
    for (bi, block) in f.blocks.iter().enumerate() {
        b.label(format!("L{bi}"));
        {
            let mut e = Emitter::new(cx, f, &mut b);
            e.stmts(&block.stmts);
            e.term(&block.terminator);
        }
    }
    m.push_func(b);
}

fn emit_fn_typedefs(m: &mut ModuleBuilder, cx: &Cx<'_>) {
    let mut seen: IndexMap<String, (CTy, Vec<CTy>)> = IndexMap::new();
    let mut add = |ty: TypeId| {
        if !matches!(cx.interner.kind(ty), TyKind::Func(..)) {
            return;
        }
        let (name, ret, params) = fn_ptr_abi(cx.interner, ty);
        seen.entry(name).or_insert((ret, params));
    };
    for f in &cx.mir.functions {
        for decl in &f.locals {
            add(decl.ty);
        }
        if f.is_async {
            if let Some(hir) = f.hir_fn.as_ref() {
                let body = crate::lower::lower_async_poll_body(hir, cx.interner);
                for decl in &body.locals {
                    add(decl.ty);
                }
                for b in &body.blocks {
                    for s in &b.stmts {
                        match s {
                            Statement::IndirectCall { sig, .. } => add(*sig),
                            Statement::Assign(_, Rvalue::IndirectCall { sig, .. }) => add(*sig),
                            _ => {}
                        }
                    }
                }
            }
        }
        for b in &f.blocks {
            for s in &b.stmts {
                match s {
                    Statement::IndirectCall { sig, .. } => add(*sig),
                    Statement::Assign(_, Rvalue::IndirectCall { sig, .. }) => add(*sig),
                    _ => {}
                }
            }
        }
    }
    for inf in &cx.mir.interfaces.interfaces {
        for &sig in &inf.sigs {
            add(sig);
        }
    }
    seen.entry("dream_fn_ptr__ptr".into())
        .or_insert((CTy::Ptr, vec![CTy::Ptr]));
    for (name, (ret, params)) in seen {
        m.push(Item::Typedef { name, ret, params });
    }
}
