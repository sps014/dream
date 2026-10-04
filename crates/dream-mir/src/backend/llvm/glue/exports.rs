use super::super::{lcx::Lcx, types};
use super::glue;

pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
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
