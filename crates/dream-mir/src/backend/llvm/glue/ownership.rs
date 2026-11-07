//! Exact heap edge metadata; ownership traversal never interprets scalar payloads as addresses.
use super::super::fx::V;
use super::super::ir::{GlobalDef, Linkage, Ty, Value};
use super::super::lcx::Lcx;
use super::{glue, register};
use crate::backend::shared::abi_types::elem_size;
use crate::backend::shared::glue::{del_symbol, glue_array_elems, struct_field_drops};
use dream_types::TypeId;

pub(in super::super) fn array_info(elem: TypeId) -> String {
    format!("dream_array_info_{}", elem.0)
}
fn info(ty: TypeId) -> String {
    format!("dream_type_info_{}", ty.0)
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
                fields: vec![Ty::Ptr, Ty::Ptr, Ty::Ptr, Ty::Ptr],
            },
            init: Some(format!(
                "{{ ptr @{visit}, ptr {fin}, ptr @{clear}, ptr @dream_recycle }}"
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
        let drops = layout.as_ref().map(|s| struct_field_drops(&l.cx, *ty, s));
        let mut fx = glue(l, &symbol(*ty, "clear"));
        let p = fx.arg(0);
        if let Some(drops) = drops {
            for d in drops {
                fx.field_drop_code(&p, d);
            }
        }
        if let Some(u) = union {
            fx.union_drops(&p, &u);
        }
        fx.w.ret(None);
        fx.finish();
        descriptor(
            l,
            &info(*ty),
            &symbol(*ty, "visit"),
            destructor.map(|_| symbol(*ty, "finalize")).as_deref(),
            &symbol(*ty, "clear"),
        );
    }
    for elem in glue_array_elems(&l.cx) {
        let es = elem_size(&l.cx, elem) as i64;
        let mut fx = glue(l, &symbol(elem, "array_visit"));
        let p = fx.arg(0);
        let pp = fx.ptr(&p);
        let n = fx.load_ty(Ty::I32, &pp, 4, false);
        fx.for_each_array_elem(&p, &n, es, |fx, _, at| {
            if fx.is_value(elem) {
                let at = fx.as_ref(&V::s(at));
                fx.visit_refs(elem, &at);
            } else if fx.is_rc(elem) {
                let child = fx.load_ty(fx.h(), &at, 4, true);
                let child = fx.as_ref(&child);
                fx.call("dream_visit_edge", &[child]);
            }
        });
        fx.w.ret(None);
        fx.finish();
        let mut fx = glue(l, &symbol(elem, "array_clear"));
        let p = fx.arg(0);
        fx.array_elems_drop(&p, elem);
        fx.w.ret(None);
        fx.finish();
        descriptor(
            l,
            &array_info(elem),
            &symbol(elem, "array_visit"),
            None,
            &symbol(elem, "array_clear"),
        );
    }
    emit_lookup(l, &types);
}

fn emit_lookup(l: &mut Lcx<'_>, types: &[TypeId]) {
    let first = crate::abi::TAG_STRUCT_BASE;
    let count = types
        .iter()
        .filter_map(|ty| l.cx.tags.get(ty))
        .max()
        .map_or(0, |&last| (last - first + 1) as usize);
    let mut entries = vec![None; count];
    for ty in types {
        if let Some(&tag) = l.cx.tags.get(ty) {
            entries[(tag - first) as usize] = Some(info(*ty));
        }
    }
    if !entries.is_empty() {
        let (ty, init) = super::tables::ptr_array(&entries);
        l.global(
            "dream_nominal_type_infos",
            GlobalDef {
                linkage: Linkage::Internal,
                thread_local: false,
                constant: true,
                unnamed_addr: false,
                ty,
                init: Some(init),
                align: l.cx.target.spec().ptr_align,
            },
        );
    }
    let mut fx = glue(l, "dream_type_info_for_tag");
    let tag = fx.arg(0);
    if count != 0 {
        let index = fx.w.bin("sub", &tag.v, &Value::i32(first as i64));
        let valid = fx.w.icmp("ult", &index, &Value::i32(count as i64));
        fx.if_then(&valid, |fx| {
            let index = fx.conv(&V::s(index), &Ty::I64);
            let abi = fx.l.cx.target.abi();
            let offset = fx.w.bin("mul", &index, &Value::i64(abi.ptr_size as i64));
            let address =
                fx.w.gep_i8(&Value::global("dream_nominal_type_infos"), &offset);
            let metadata = fx.w.load(Ty::Ptr, &address, abi.ptr_align, &[]);
            let present = fx.w.icmp("ne", &metadata, &Value::null());
            fx.if_then(&present, |fx| fx.w.ret(Some(&metadata)));
        });
    }
    let builtin = fx.call_v("dream_builtin_type_info", &[tag]);
    fx.w.ret(Some(&builtin.v));
    fx.finish();
}
