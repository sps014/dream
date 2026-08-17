use super::builder::{ModuleBuilder, ValType};
use super::*;

/// The emission-driving lookup tables derived once per module and shared by every per-function emit:
/// resolved call symbols, callee signatures (for argument widening), the interned-string address map,
/// struct/union type tags, the indirect-call function table, and the set of value structs needing
/// retain/drop glue. Built by [`build_tables`] so [`emit_program`] and [`emit_module_with_debug`]
/// derive them identically.
struct ModuleTables {
    symbols: HashMap<(DefId, Vec<TypeId>), String>,
    sigs: HashMap<(DefId, Vec<TypeId>), Vec<TypeId>>,
    strings: IndexMap<String, u32>,
    tags: HashMap<TypeId, i32>,
    ftable: IndirectTable,
    value_glue: std::collections::HashSet<TypeId>,
    intrinsic_ops: HashMap<DefId, dream_abi::intrinsics::IntrinsicOp>,
}

/// Derives the shared [`ModuleTables`] for `mir`.
///
/// `locate_panics` controls whether runtime check messages include file:line (debug / debug-info)
/// or share four compact base strings (release).
fn build_tables(mir: &crate::Mir, interner: &TypeInterner, locate_panics: bool) -> ModuleTables {
    ModuleTables {
        symbols: symbol_table(mir),
        sigs: signature_table(mir, interner),
        strings: string_table(mir, interner, locate_panics),
        tags: struct_tags(mir),
        ftable: func_table(mir),
        value_glue: value_glue_types(mir, interner),
        intrinsic_ops: intrinsic_ops(mir),
    }
}

/// Emits a whole MIR program as a sequence of WAT function definitions (no module wrapper). Used by
/// the pipeline tests; the driver target is [`emit_module`].
pub fn emit_program(mir: &crate::Mir, interner: &TypeInterner) -> String {
    emit_module(mir, interner, true)
}

/// Emits a whole MIR program as a single `(module ...)`, exporting every (non-instance) function
/// under its source name. This is the self-contained unit the driver hands to the WASM assembler.
pub fn emit_module(mir: &crate::Mir, interner: &TypeInterner, debug: bool) -> String {
    let has_main = mir
        .functions
        .iter()
        .any(|f| f.instance.is_empty() && f.name == crate::abi::ENTRY_FN);
    emit_module_with_debug(mir, interner, debug, false, debug || !has_main).0
}

/// Binary module bytes plus optional debug map. [`emit_module_with_debug`] pretty-prints WAT from this.
pub fn emit_module_bytes(
    mir: &crate::Mir,
    interner: &TypeInterner,
    debug: bool,
    debug_info: bool,
    export_user_fns: bool,
) -> (Vec<u8>, Option<crate::emit::debug_map::DebugModule>) {
    emit_module_encoded(mir, interner, debug, debug_info, export_user_fns)
}

/// Like [`emit_module`], but when `debug_info` is set it also instruments every function with the
/// `dream_debug` source-line hooks + local spilling and returns the [`DebugModule`] source map
/// describing them. When `debug_info` is false the returned map is `None` and the WAT is identical
/// to [`emit_module`]. `export_user_fns` exports every non-generic user function (debug / `-g` /
/// `--crate-type lib`); release binaries export only `main` plus the host ABI.
pub fn emit_module_with_debug(
    mir: &crate::Mir,
    interner: &TypeInterner,
    debug: bool,
    debug_info: bool,
    export_user_fns: bool,
) -> (String, Option<crate::emit::debug_map::DebugModule>) {
    let (bytes, map) = emit_module_encoded(mir, interner, debug, debug_info, export_user_fns);
    (super::builder::print_wasm(&bytes), map)
}

fn emit_module_encoded(
    mir: &crate::Mir,
    interner: &TypeInterner,
    debug: bool,
    debug_info: bool,
    export_user_fns: bool,
) -> (Vec<u8>, Option<crate::emit::debug_map::DebugModule>) {
    let locate_panics = debug || debug_info;
    let ModuleTables {
        symbols,
        sigs,
        strings,
        tags,
        ftable,
        value_glue,
        intrinsic_ops,
    } = build_tables(mir, interner, locate_panics);
    let global_tys: HashMap<u32, TypeId> = mir.globals.iter().map(|g| (g.id.0, g.ty)).collect();
    let defined = defined_functions(mir);

    let dbg_module = if debug_info {
        Some(crate::emit::debug_map::DebugModule::build(
            mir, interner, &symbols,
        ))
    } else {
        None
    };
    let dbg_by_symbol: HashMap<&str, &crate::emit::debug_map::DebugFunction> = dbg_module
        .as_ref()
        .map(|m| m.functions.iter().map(|f| (f.symbol.as_str(), f)).collect())
        .unwrap_or_default();

    let mut m = ModuleBuilder::new();
    m.dce = !debug && !debug_info;

    emit_imports_builder(&mut m, mir, interner);
    if debug_info {
        let dm = crate::emit::debug_map::DEBUG_MODULE;
        m.import_func(
            dm,
            "line",
            "__dbg_line",
            vec![ValType::I32, ValType::I32],
            vec![],
        );
        m.import_func(dm, "enter", "__dbg_enter", vec![ValType::I32], vec![]);
        m.import_func(dm, "exit", "__dbg_exit", vec![ValType::I32], vec![]);
    }

    intern_func_signatures(&mut m, interner);
    let n = ftable.elem.len();
    m.table("__ft", (n + 1) as u32, (n + 1) as u32);
    if n > 0 {
        m.elem(
            "__ft",
            1,
            ftable
                .elem
                .iter()
                .map(|s| s.trim_start_matches('$').to_string())
                .collect(),
        );
    }
    m.export_table("__indirect_function_table", "__ft");

    let used_slots = used_iface_slots(mir);
    let (vg_addrs, static_end) = value_global_addrs(mir, interner, heap_base(&strings));
    let iface = emit_interface_dispatch(mir, interner, static_end, &used_slots, &ftable.slots);

    let data_end = iface.heap_start;
    let heap_base = data_end + SHADOW_STACK_SIZE;
    let initial_pages = heap_base.div_ceil(WASM_PAGE_SIZE) + INITIAL_HEAP_PAGES;
    m.import_memory("env", "memory", initial_pages, crate::abi::MAX_MEMORY_PAGES);

    m.global_i32("free_list_head", true, 0);
    m.global_i32("__tid", true, 0);
    m.global_i32("__sp", true, heap_base as i32);
    m.global_i32("live_objects", true, 0);
    m.global_i32("total_allocations", true, 0);

    for g in &mir.globals {
        if let Some(&addr) = vg_addrs.get(&g.id.0) {
            m.global_i32(&format!("g{}", g.id.0), true, addr as i32);
        } else {
            let ty = wasm_val_ty(interner, g.ty);
            m.global(&format!("g{}", g.id.0), ty, true, 0, false, false);
        }
    }

    if let Some(dbg) = &dbg_module {
        for k in 0..dbg.global_pool {
            let name = format!("__dbg_v{k}");
            m.global(&name, ValType::I64, true, 0, false, false);
            m.export_global(&name, &name);
        }
    }

    m.ingest_wat(&runtime_prelude(
        debug,
        module_needs_threads(mir),
        strings[""],
    ));
    m.ingest_wat(RUNTIME_WEAK);
    m.ingest_wat(RUNTIME_CLOSURE);
    m.ingest_wat(&RUNTIME_SYNC.replace(
        "{THREAD_ID_COUNTER_ADDR}",
        &crate::abi::THREAD_ID_COUNTER_ADDR.to_string(),
    ));
    if crate::async_emit::module_has_async(&mir.functions) {
        m.ingest_wat(&crate::async_emit::async_runtime_wat());
    }
    m.ingest_wat(&to_string_runtime(&strings));
    m.ingest_wat(RUNTIME_PANIC);

    let mut proto = String::new();
    emit_object_protocol(&mut proto, mir, interner, &strings, &tags);
    m.ingest_wat(&proto);
    let mut js = String::new();
    emit_js_marshal(&mut js, mir, interner, &strings, &tags);
    m.ingest_wat(&js);
    let mut rel = String::new();
    emit_release_funcs(&mut rel, mir, interner, &tags, &value_glue);
    m.ingest_wat(&rel);
    let mut glue = String::new();
    emit_value_glue(&mut glue, mir, interner, &value_glue);
    m.ingest_wat(&glue);
    m.ingest_wat(&iface.trampolines);

    for (s, addr) in &strings {
        let block = addr - HEAP_HEADER_SIZE;
        m.data(block, string_block_bytes(s));
    }
    m.ingest_wat(&iface.data);

    let mut has_init = false;
    for f in &mir.functions {
        if f.is_async {
            let debug_fn = dbg_by_symbol.get(func_symbol(f).as_str()).copied();
            let (ctor, poll) = crate::async_emit::emit_async_function_parts(
                f,
                interner,
                &symbols,
                &mir.layouts,
                &strings,
                &tags,
                &ftable.slots,
                &defined,
                &value_glue,
                *ftable.polls.get(&(f.def, f.instance.clone())).unwrap_or(&0),
                debug,
                locate_panics,
                debug_fn,
                &intrinsic_ops,
            );
            m.ingest_wat(&ctor);
            m.push_func(poll);
        } else {
            let debug_fn = dbg_by_symbol.get(func_symbol(f).as_str()).copied();
            m.push_func(emit_function_with(
                f,
                interner,
                &symbols,
                &sigs,
                &mir.layouts,
                &strings,
                &tags,
                &ftable.slots,
                &value_glue,
                &global_tys,
                &defined,
                debug,
                locate_panics,
                debug_fn,
                &intrinsic_ops,
            ));
        }
        if f.name == crate::lower::INIT_FN_NAME {
            has_init = true;
        } else if f.instance.is_empty() && f.name == crate::abi::ENTRY_FN && f.is_async {
            m.ingest_wat(&crate::async_emit::emit_async_main_wrapper(
                &func_symbol(f),
                !f.params.is_empty(),
            ));
        } else if f.instance.is_empty() && f.name == crate::abi::ENTRY_FN && !f.params.is_empty() {
            m.ingest_wat(&format!(
                "(func (export \"main\")\n (local $args i32)\n i32.const 4\n i32.const {}\n call $malloc\n local.set $args\n local.get $args\n i32.const 0\n i32.store\n local.get $args\n call ${}\n)",
                crate::abi::TAG_ARRAY,
                func_symbol(f),
            ));
        } else if f.instance.is_empty() && (export_user_fns || f.name == crate::abi::ENTRY_FN) {
            m.export_func(&f.name, &func_symbol(f));
        }
    }

    m.intern_type(Some("__worker_sig"), vec![ValType::I32], vec![ValType::I32]);
    let has_closure_env = mir.globals.iter().any(|g| g.id.0 == 0);
    let has_async_runtime = crate::async_emit::module_has_async(&mir.functions);
    let publish_env = if has_closure_env {
        " local.get $env\n global.set $g0\n"
    } else {
        ""
    };
    m.ingest_wat(&format!(
        "(func $__dream_worker_invoke_raw (param $fn i32) (param $env i32) (param $arg i32) (result i32)\n{publish_env} local.get $arg\n local.get $fn\n call_indirect $__ft (type $__worker_sig))"
    ));
    if has_async_runtime {
        m.ingest_wat(&format!(
            "(func $__dream_worker_invoke (param $fn i32) (param $env i32) (param $arg i32) (result i32)\n (local $r i32)\n local.get $fn\n local.get $env\n local.get $arg\n call $__dream_worker_invoke_raw\n local.set $r\n local.get $r\n i32.const 0\n i32.ne\n local.get $r\n call $object_tag\n i32.eqz\n i32.and\n (if\n  (then\n   call $dream_run_loop\n   local.get $r\n   i32.load offset={}\n   local.set $r\n  )\n )\n local.get $r)",
            crate::async_emit::F_RESULT,
        ));
    } else {
        m.ingest_wat(
            "(func $__dream_worker_invoke (param $fn i32) (param $env i32) (param $arg i32) (result i32)\n local.get $fn\n local.get $env\n local.get $arg\n call $__dream_worker_invoke_raw)",
        );
    }

    let mut init = String::from("(func $__runtime_init\n");
    init.push_str(&format!(
        "  i32.const {}\n  i32.const 0\n  i32.const {}\n  i32.atomic.rmw.cmpxchg\n  drop\n",
        crate::abi::HEAP_PTR_ADDR,
        heap_base
    ));
    if has_init {
        init.push_str(&format!("  call ${}\n", crate::lower::INIT_FN_NAME));
    }
    init.push_str(")\n");
    m.ingest_wat(&init);
    m.set_start("__runtime_init");

    use crate::abi;
    m.export_memory(abi::EXPORT_MEMORY);
    m.export_func(abi::EXPORT_MALLOC, "malloc");
    m.export_func(abi::EXPORT_FREE, "free");
    m.export_func(abi::EXPORT_WORKER_INVOKE, "__dream_worker_invoke");
    m.export_func(abi::EXPORT_WORKER_INVOKE_RAW, "__dream_worker_invoke_raw");
    if crate::async_emit::module_has_async(&mir.functions) {
        m.export_func(abi::EXPORT_RUN_LOOP, "dream_run_loop");
        m.export_func(abi::EXPORT_RESOLVE, "dream_resolve");
        m.export_func(abi::EXPORT_NEW_FUTURE, "dream_new_future");
    }

    (m.finish(), dbg_module)
}

fn intern_func_signatures(m: &mut ModuleBuilder, interner: &TypeInterner) {
    let mut seen: IndexMap<String, (Vec<ValType>, Vec<ValType>)> = IndexMap::new();
    for (id, kind) in interner.iter_kinds() {
        if matches!(kind, TyKind::Func(..)) {
            if let Some((name, ptys, rty)) = func_sig(interner, id) {
                let params: Vec<ValType> = ptys.iter().map(|t| parse_val(t)).collect();
                let results = rty.map(|t| vec![parse_val(t)]).unwrap_or_default();
                seen.entry(name.trim_start_matches('$').to_string())
                    .or_insert((params, results));
            }
        }
    }
    for (name, (params, results)) in seen {
        m.intern_type(Some(&name), params, results);
    }
}

fn parse_val(t: &str) -> ValType {
    match t {
        "i64" => ValType::I64,
        "f32" => ValType::F32,
        "f64" => ValType::F64,
        "v128" => ValType::V128,
        _ => ValType::I32,
    }
}

/// Emits the module's `(import ...)` declarations: the fixed host `print_*` builtins (which
/// `print`/`println` lower to) followed by user `extern fun` interop imports. When any Dream `js*`
/// bridge or GPU import is live, also emit `$js_retain` / `$js_release` for host-side handle RC
/// (compiler-emitted — not declared in the stdlib prelude). Call sites reference each import's
/// internal `$name`; the `module`/`field` pair names the host binding.
pub(super) fn emit_imports_builder(
    m: &mut ModuleBuilder,
    mir: &crate::Mir,
    interner: &TypeInterner,
) {
    for (name, param) in [
        ("print_string", ValType::I32),
        ("print_int", ValType::I32),
        ("print_float", ValType::F32),
        ("print_double", ValType::F64),
        ("print_char", ValType::I32),
    ] {
        m.import_func(crate::abi::ENV_MODULE, name, name, vec![param], vec![]);
    }
    let needs_js_rc = mir
        .imports
        .iter()
        .any(|imp| imp.field.starts_with("js") || imp.field.starts_with("gpu"));
    for imp in &mir.imports {
        if imp.field == "jsRelease" || imp.field == "jsRetain" {
            continue;
        }
        let params: Vec<ValType> = imp
            .params
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let by_ref = imp.param_by_ref.get(i).copied().unwrap_or(false);
                if by_ref {
                    ValType::I32
                } else {
                    wasm_val_ty(interner, *t)
                }
            })
            .collect();
        let results = match imp.ret {
            Some(t) => vec![wasm_val_ty(interner, t)],
            None => vec![],
        };
        m.import_func(&imp.module, &imp.field, &imp.name, params, results);
    }
    if needs_js_rc {
        m.import_func("Dream", "jsRetain", "js_retain", vec![ValType::I32], vec![]);
        m.import_func(
            "Dream",
            "jsRelease",
            "js_release",
            vec![ValType::I32],
            vec![],
        );
    }
}
