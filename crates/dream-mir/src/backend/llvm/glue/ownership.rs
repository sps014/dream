//! Exact heap edge metadata; ownership traversal never interprets scalar payloads as addresses.
use super::super::fx::V;
use super::super::ir::{GlobalDef, Linkage, Ty, Value};
use super::super::lcx::Lcx;
use super::{glue, register};
use crate::backend::shared::abi_types::elem_size;
use crate::backend::shared::glue::{del_symbol, glue_array_elems, struct_field_drops};
use dream_types::{TyKind, TypeId};

pub(in super::super) fn array_info(elem: TypeId) -> String {
    format!("dream_array_info_{}", elem.0)
}
fn info(ty: TypeId) -> String {
    format!("dream_type_info_{}", ty.0)
}
/// The `cycle_capable = 0` twin a proven-private (`AllocPolicy::Private`) region allocation uses.
pub(crate) fn untracked_info(ty: TypeId) -> String {
    format!("dream_type_info_untracked_{}", ty.0)
}
fn symbol(ty: TypeId, action: &str) -> String {
    format!("dream_{action}_{}", ty.0)
}
fn types(l: &Lcx<'_>) -> Vec<TypeId> {
    l.mir
        .layouts
        .structs
        .keys()
        .chain(l.mir.layouts.unions.keys())
        .copied()
        .collect()
}
pub(in super::super) fn register_all(l: &mut Lcx<'_>) {
    register(l, "dream_type_info_for_tag", Ty::Ptr, vec![Ty::I32]);
    for ty in types(l) {
        for action in ["visit", "finalize", "clear"] {
            register(l, &symbol(ty, action), Ty::Void, vec![l.h()]);
        }
    }
    for elem in glue_array_elems(&l.cx) {
        for action in ["array_visit", "array_clear"] {
            register(l, &symbol(elem, action), Ty::Void, vec![l.h()]);
        }
    }
}
pub(in super::super) fn descriptor(
    l: &mut Lcx<'_>,
    name: &str,
    visit: &str,
    finalize: Option<&str>,
    clear: &str,
    cyclic: bool,
    pure_clear: bool,
) {
    for f in [Some(visit), finalize, Some(clear), Some("dream_recycle")]
        .into_iter()
        .flatten()
    {
        l.fn_ref(f);
    }
    let fin = finalize.map_or_else(|| "null".into(), |f| format!("@{f}"));
    l.global(
        name,
        GlobalDef {
            linkage: Linkage::Internal,
            thread_local: false,
            constant: true,
            unnamed_addr: false,
            ty: Ty::Struct {
                packed: false,
                fields: vec![Ty::Ptr, Ty::Ptr, Ty::Ptr, Ty::Ptr, Ty::I32, Ty::I32],
            },
            init: Some(format!(
                "{{ ptr @{visit}, ptr {fin}, ptr @{clear}, ptr @dream_recycle, i32 {}, i32 {} }}",
                i32::from(cyclic),
                i32::from(pure_clear)
            )),
            align: l.cx.target.spec().ptr_align,
        },
    );
}
pub(in super::super) fn emit_all(l: &mut Lcx<'_>) {
    let types = types(l);
    for ty in &types {
        let mut fx = glue(l, &symbol(*ty, "visit"));
        let p = fx.arg(0);
        fx.visit_refs(*ty, &p);
        fx.w.ret(None);
        fx.finish();
        let destructor = l.cx.nstruct(*ty).and_then(|s| s.destructor);
        let mut fx = glue(l, &symbol(*ty, "finalize"));
        if let Some(def) = destructor {
            let del = del_symbol(&fx.l.cx, def);
            let p = fx.arg(0);
            fx.call(&del, &[p]);
        }
        fx.w.ret(None);
        fx.finish();
        let layout = l.cx.nstruct(*ty).cloned();
        let union = l.cx.nunion(*ty).cloned();
        let drops = layout.as_ref().map(|s| struct_field_drops(&l.cx, s));
        // Reference drops defer child finalizers through the collector queue. Inline value
        // drops may execute user destructors directly and must never run under this gate.
        let batched_clear = crate::ownership::cycle_capable(&l.mir.layouts, l.interner, *ty)
            && layout.as_ref().is_some_and(|s| {
                s.fields.iter().all(|field| {
                    field.is_weak || field.is_unowned || !l.interner.is_value_type(field.ty)
                })
            });
        let mut fx = glue(l, &symbol(*ty, "clear"));
        let p = fx.arg(0);
        if batched_clear {
            fx.call("dream_cycle_enter", &[]);
        }
        if let Some(drops) = drops {
            for d in drops {
                fx.field_drop_code(&p, d);
            }
        }
        if let Some(u) = union {
            fx.union_drops(&p, &u);
        }
        if batched_clear {
            fx.call("dream_cycle_leave", &[]);
        }
        fx.w.ret(None);
        fx.finish();
        let cyclic = crate::ownership::cycle_capable(&l.mir.layouts, l.interner, *ty);
        // Only class instances can be private region allocations (`AllocPolicy::Private`).
        if cyclic && layout.is_some() {
            descriptor(
                l,
                &untracked_info(*ty),
                &symbol(*ty, "visit"),
                destructor.map(|_| symbol(*ty, "finalize")).as_deref(),
                &symbol(*ty, "clear"),
                false,
                batched_clear,
            );
        }
        descriptor(
            l,
            &info(*ty),
            &symbol(*ty, "visit"),
            destructor.map(|_| symbol(*ty, "finalize")).as_deref(),
            &symbol(*ty, "clear"),
            cyclic,
            batched_clear,
        );
    }
    for elem in glue_array_elems(&l.cx) {
        let es = elem_size(&l.cx, elem) as i64;
        let mut fx = glue(l, &symbol(elem, "array_visit"));
        let p = fx.arg(0);
        let pp = fx.ptr(&p);
        let n = fx.load_ty(Ty::I32, &pp, 4, false);
        let n = fx.conv(&n, &Ty::I64);
        let slot = fx.w.alloca(Ty::I64, 8);
        fx.w.store(&Value::i64(0), &slot, 8, &[]);
        let head = fx.w.new_block("edge.head");
        let body = fx.w.new_block("edge.body");
        let done = fx.w.new_block("edge.done");
        fx.w.br(head);
        fx.w.switch_to(head);
        let i = fx.w.load(Ty::I64, &slot, 8, &[]);
        let lt = fx.w.icmp("slt", &i, &n);
        fx.w.cond_br(&lt, body, done);
        fx.w.switch_to(body);
        let off = fx.w.bin("mul", &i, &Value::i64(es));
        let off = fx.w.bin("add", &off, &Value::i64(4));
        let at = fx.addr_dyn(&p, &off);
        if fx.is_value(elem) {
            let at = fx.as_ref(&V::s(at));
            fx.visit_refs(elem, &at);
        } else if fx.is_rc(elem) {
            let child = fx.load_ty(fx.h(), &at, 4, true);
            let child = fx.as_ref(&child);
            fx.call("dream_visit_edge", &[child]);
        }
        let inc = fx.w.bin("add", &i, &Value::i64(1));
        fx.w.store(&inc, &slot, 8, &[]);
        fx.w.br(head);
        fx.w.switch_to(done);
        fx.w.ret(None);
        fx.finish();
        let mut fx = glue(l, &symbol(elem, "array_clear"));
        let p = fx.arg(0);
        fx.array_elems_drop(&p, elem);
        fx.w.ret(None);
        fx.finish();
        // Erased arrays can be put into themselves or into their element's graph.
        let cyclic = l
            .interner
            .lookup(&TyKind::Array(elem))
            .is_none_or(|ty| crate::ownership::cycle_capable(&l.mir.layouts, l.interner, ty));
        descriptor(
            l,
            &array_info(elem),
            &symbol(elem, "array_visit"),
            None,
            &symbol(elem, "array_clear"),
            cyclic,
            !l.interner.is_value_type(elem),
        );
    }
    let arms: Vec<_> = types
        .iter()
        .filter_map(|ty| l.cx.tags.get(ty).map(|tag| (*tag as i128, info(*ty))))
        .collect();
    let mut fx = glue(l, "dream_type_info_for_tag");
    let tag = fx.arg(0);
    let default = fx.w.new_block("builtin");
    let blocks: Vec<_> = arms
        .iter()
        .map(|(tag, name)| (*tag, fx.w.new_block("type"), name))
        .collect();
    let switch: Vec<_> = blocks
        .iter()
        .map(|(tag, block, _)| (*tag, *block))
        .collect();
    fx.w.switch(&tag.v, default, &switch);
    for (_, block, name) in blocks {
        fx.w.switch_to(block);
        fx.w.ret(Some(&Value::global(name)));
    }
    fx.w.switch_to(default);
    let builtin = fx.call_v("dream_builtin_type_info", &[tag]);
    fx.w.ret(Some(&builtin.v));
    fx.finish();
}
