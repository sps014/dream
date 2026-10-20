use super::super::{lcx::Lcx, types};
use super::glue;

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    if needs_guest_context(l) {
        for (export, target) in context_exports(l) {
            let sig = l.sig(target);
            l.own(export, sig);
            l.export(export, export);
        }
    }
    for (def, export) in &l.mir.exports {
        let f = l
            .mir
            .functions
            .iter()
            .find(|f| f.def == *def)
            .unwrap_or_else(|| crate::internal_error!("exported function was pruned"));
        let sig = types::fn_ll_sig(l.interner, f, &l.h(), &l.word());
        l.own(export, sig);
        l.export(export, export);
    }
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    if needs_guest_context(l) {
        for (export, target) in context_exports(l) {
            let count = l.sig(target).fty.params.len();
            let mut fx = glue(l, export);
            let args = (0..count).map(|i| fx.arg(i)).collect::<Vec<_>>();
            let result = fx.call(target, &args);
            fx.w.ret(result.as_ref().map(|v| &v.v));
            fx.finish();
        }
    }
    for (def, export) in &l.mir.exports {
        let f = l
            .mir
            .functions
            .iter()
            .find(|f| f.def == *def)
            .unwrap_or_else(|| crate::internal_error!("exported function was pruned"));
        let target = l.abi_sym(&l.user_fn(f));
        let mut fx = glue(l, export);
        fx.call("dream_runtime_init", &[]);
        if !fx.l.cx.target.spec().capabilities.linear_memory {
            fx.call("dream_callback_enter", &[]);
        }
        let args = (0..f.params.len()).map(|i| fx.arg(i)).collect::<Vec<_>>();
        let result = fx.call(&target, &args);
        fx.w.ret(result.as_ref().map(|v| &v.v));
        fx.finish();
    }
}

fn needs_guest_context(l: &Lcx<'_>) -> bool {
    l.cx.target.spec().capabilities.js_interop
        && (!l.mir.imports.is_empty() || !l.mir.exports.is_empty())
}

fn context_exports(l: &Lcx<'_>) -> Vec<(&'static str, &'static str)> {
    let mut exports = dream_abi::guest_contract::EXPORTS.to_vec();
    if l.mir.imports.iter().any(|import| {
        import
            .params
            .iter()
            .any(|ty| matches!(l.interner.kind(*ty), dream_types::TyKind::Func(..)))
    }) {
        exports.push(dream_abi::guest_contract::FUNCTION_EXPORT);
    }
    if l.mir
        .imports
        .iter()
        .any(|import| import.module == "Dream" && import.field == "gpuLibraryLoad")
    {
        exports.extend([
            ("__dream_gpu_version", "dream_gpu_bundle_abi_version"),
            ("__dream_gpu_index", "dream_gpu_bundle_index"),
        ]);
    }
    exports
}
