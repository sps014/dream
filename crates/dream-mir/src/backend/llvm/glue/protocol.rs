//! Generated `to_string` / `hash_code` bodies for structs, unions and arrays, and the tag-routed
//! `object` protocol functions.

use super::super::fx::{Fx, V};
use super::super::ir::{Ty, Value};
use super::super::lcx::Lcx;
use super::super::places::ELEM_ALIGN;
use super::{glue, register};
use crate::abi;
use crate::backend::shared::abi_types::{c_ident, elem_size, mem_ty};
use crate::backend::shared::func_symbol;
use crate::backend::shared::protocol_names::to_string_fn;
use crate::backend::shared::reach::ProtocolReach;
use crate::backend::shared::tables::{BUILTIN_TYPE_NAMES, NULL_TYPE_NAME, UNKNOWN_TYPE_NAME};
use dream_hir::FieldLayout;
use dream_types::{PrimTy, TyKind, TypeId};
use indexmap::IndexSet as HashSet;

fn builtin_tag(name: &str) -> i32 {
    match name {
        "TAG_INT" => abi::TAG_INT,
        "TAG_UINT" => abi::TAG_UINT,
        "TAG_LONG" => abi::TAG_LONG,
        "TAG_ULONG" => abi::TAG_ULONG,
        "TAG_BYTE" => abi::TAG_BYTE,
        "TAG_BOOL" => abi::TAG_BOOL,
        "TAG_CHAR" => abi::TAG_CHAR,
        "TAG_FLOAT" => abi::TAG_FLOAT,
        "TAG_DOUBLE" => abi::TAG_DOUBLE,
        "TAG_STRING" => abi::TAG_STRING,
        "TAG_ARRAY" => abi::TAG_ARRAY,
        "TAG_FUNCBOX" => abi::TAG_FUNCBOX,
        "TAG_FUTURE" => abi::TAG_FUTURE,
        other => crate::internal_error!("unknown builtin tag {other}"),
    }
}

/// Everything the protocol pass emits, decided once so registration and emission agree.
pub(in super::super) struct Plan {
    arrays: Vec<TypeId>,
    struct_to_string: Vec<(TypeId, String)>,
    union_to_string: Vec<(TypeId, String)>,
    struct_hash: Vec<(TypeId, String)>,
    union_hash: Vec<(TypeId, String)>,
    type_name: bool,
    dynamic: bool,
}

pub(in super::super) fn plan(l: &Lcx<'_>, reach: &ProtocolReach) -> Plan {
    let cx = &l.cx;
    let mut arrays: Vec<TypeId> = cx
        .mir
        .functions
        .iter()
        .flat_map(|f| f.locals.iter().map(|d| d.ty))
        .chain(
            cx.native
                .structs
                .values()
                .flat_map(|layout| layout.fields.iter().map(|f| f.ty)),
        )
        .filter_map(|ty| match cx.interner.kind(ty) {
            TyKind::Array(e) => Some(*e),
            _ => None,
        })
        .filter(|e| reach.needs_to_string(*e))
        .collect();
    arrays.sort_by_key(|t| t.0);
    arrays.dedup();
    let user: HashSet<String> = cx.mir.functions.iter().map(func_symbol).collect();
    let pick = |name: &str, suffix: &str| {
        let sym = format!("{name}{suffix}");
        (!user.contains(&sym)).then(|| c_ident(&sym))
    };
    let mut p = Plan {
        arrays,
        struct_to_string: Vec::new(),
        union_to_string: Vec::new(),
        struct_hash: Vec::new(),
        union_hash: Vec::new(),
        type_name: cx.mir.uses_type_name,
        dynamic: reach.dynamic,
    };
    for (ty, layout) in &cx.native.structs {
        if reach.needs_to_string(*ty) {
            if let Some(s) = pick(&layout.name, "_to_string") {
                p.struct_to_string.push((*ty, s));
            }
        }
        if reach.needs_hash_code(*ty) {
            if let Some(s) = pick(&layout.name, "_hash_code") {
                p.struct_hash.push((*ty, s));
            }
        }
    }
    for (ty, layout) in &cx.native.unions {
        if reach.needs_to_string(*ty) || reach.to_string.contains(ty) {
            if let Some(s) = pick(&layout.name, "_to_string") {
                p.union_to_string.push((*ty, s));
            }
        }
        if reach.needs_hash_code(*ty) {
            if let Some(s) = pick(&layout.name, "_hash_code") {
                p.union_hash.push((*ty, s));
            }
        }
    }
    p
}

pub(in super::super) fn register_all(l: &mut Lcx<'_>, p: &Plan) {
    for e in &p.arrays {
        let h = l.h();
        register(
            l,
            &c_ident(&format!("array_to_string_t{}", e.0)),
            h.clone(),
            vec![h],
        );
    }
    for (_, s) in p.struct_to_string.iter().chain(&p.union_to_string) {
        let h = l.h();
        register(l, s, h.clone(), vec![h]);
    }
    for (_, s) in p.struct_hash.iter().chain(&p.union_hash) {
        let h = l.h();
        register(l, s, Ty::I32, vec![h]);
    }
    if p.type_name {
        let h = l.h();
        register(l, "dream_object_type_name", h.clone(), vec![h]);
    }
    if p.dynamic {
        let h = l.h();
        register(l, "dream_object_to_string", h.clone(), vec![h.clone()]);
        register(l, "dream_object_hash_code", Ty::I32, vec![h.clone()]);
        register(l, "dream_print_object", Ty::Void, vec![h]);
    }
}

fn elem_units_bound(l: &Lcx<'_>, elem: TypeId) -> i64 {
    match l.interner.kind(elem) {
        TyKind::Prim(PrimTy::Byte | PrimTy::Bool) => 6,
        TyKind::Prim(PrimTy::Char) => 4,
        TyKind::Prim(PrimTy::Int | PrimTy::UInt) | TyKind::Enum(_) => 14,
        TyKind::Prim(PrimTy::Long | PrimTy::ULong) => 24,
        TyKind::Prim(PrimTy::Float | PrimTy::Double) => 16,
        _ => 8,
    }
}

/// `dream_strb` is `{ ptr, i32, i32 }`-sized: three zero-initialized words.
const STRB_SIZE: u64 = 16;

impl<'l, 'a> Fx<'l, 'a> {
    fn strb_new(&mut self) -> V {
        let sb = self.alloca_bytes(STRB_SIZE, 8);
        self.memset0(&sb, &Value::i64(STRB_SIZE as i64));
        V::s(sb)
    }

    fn strb_lit(&mut self, sb: &V, s: &str) {
        let lit = self.str_v(s);
        self.call("dream_strb_append", &[sb.clone(), lit]);
    }

    /// Appends `value` rendered by `conv` (the value itself when `conv` is empty) and releases
    /// the temporary rendering.
    fn strb_conv(&mut self, sb: &V, conv: &str, value: V) {
        if conv.is_empty() {
            self.call("dream_strb_append", &[sb.clone(), value]);
            return;
        }
        let t = self.call_v(conv, &[value]);
        self.call("dream_strb_append", &[sb.clone(), t.clone()]);
        self.call("dream_release", &[t]);
    }

    fn strb_finish(&mut self, sb: &V) {
        let r = self.call_v("dream_strb_finish", std::slice::from_ref(sb));
        let r = self.as_ref(&r);
        self.w.ret(Some(&r.v));
    }

    /// A field of the protocol receiver `p`: inline values by address, the rest loaded.
    fn proto_field(&mut self, p: &V, f: &FieldLayout) -> V {
        let at = self.addr(p, f.offset as i64);
        if self.is_value(f.ty) {
            return self.as_ref(&V::s(at));
        }
        let m = mem_ty(&self.l.cx, f.ty);
        let align = super::super::fx::align_at(&self.h(), f.offset as i64).min(ELEM_ALIGN);
        self.load_mem(m, &at, align)
    }

    fn ret_str(&mut self, s: &str) {
        let v = self.str_v(s);
        self.w.ret(Some(&v.v));
    }

    fn ret_null_str_if_null(&mut self, p: &V) {
        let z = self.w.icmp("eq", &p.v, &Value::i64(0));
        self.if_then(&z, |fx| fx.ret_str("null"));
    }
}

pub(in super::super) fn emit_all(l: &mut Lcx<'_>, p: &Plan) {
    for e in &p.arrays {
        array_to_string(l, *e);
    }
    for (ty, name) in &p.struct_to_string {
        struct_to_string(l, *ty, name);
    }
    for (ty, name) in &p.union_to_string {
        union_to_string(l, *ty, name);
    }
    for (ty, name) in &p.struct_hash {
        struct_hash(l, *ty, name);
    }
    for (ty, name) in &p.union_hash {
        union_hash(l, *ty, name);
    }
    if p.type_name {
        type_name_router(l);
    }
    if p.dynamic {
        to_string_router(l);
        hash_code_router(l);
        let mut fx = glue(l, "dream_print_object");
        let p = fx.arg(0);
        let s = fx.call_v("dream_object_to_string", &[p]);
        fx.call("print_string", &[s]);
        fx.w.ret(None);
        fx.finish();
    }
}

fn array_to_string(l: &mut Lcx<'_>, elem: TypeId) {
    let es = elem_size(&l.cx, elem) as i64;
    let conv = to_string_fn(&l.cx, elem);
    let bound = elem_units_bound(l, elem);
    let mut fx = glue(l, &c_ident(&format!("array_to_string_t{}", elem.0)));
    let p = fx.arg(0);
    let nz = fx.truthy(&p);
    let n = fx.if_else_v(
        &nz,
        |fx| {
            let pp = fx.ptr(&p);
            fx.load_ty(Ty::I32, &pp, 4, false)
        },
        |_| V::i32(0),
    );
    let n64 = fx.conv(&n, &Ty::I64);
    let sb = fx.strb_new();
    let reserve = fx.w.bin("mul", &n64, &Value::i64(bound));
    let reserve = fx.w.bin("add", &reserve, &Value::i64(2));
    fx.call("dream_strb_reserve", &[sb.clone(), V::s(reserve)]);
    fx.strb_lit(&sb, "[");
    let slot = fx.w.alloca(Ty::I64, 8);
    fx.w.store(&Value::i64(0), &slot, 8, &[]);
    let head = fx.w.new_block("head");
    let body = fx.w.new_block("body");
    let done = fx.w.new_block("done");
    fx.w.br(head);
    fx.w.switch_to(head);
    let i = fx.w.load(Ty::I64, &slot, 8, &[]);
    let lt = fx.w.icmp("slt", &i, &n64);
    fx.w.cond_br(&lt, body, done);
    fx.w.switch_to(body);
    let not_first = fx.w.icmp("ne", &i, &Value::i64(0));
    fx.if_then(&not_first, |fx| fx.strb_lit(&sb, ", "));
    let off = fx.w.bin("mul", &i, &Value::i64(es));
    let off =
        fx.w.bin("add", &off, &Value::i64(abi::LEN_PREFIX_SIZE as i64));
    let at = fx.addr_dyn(&p, &off);
    let value = if fx.is_value(elem) {
        fx.as_ref(&V::s(at))
    } else {
        let m = mem_ty(&fx.l.cx, elem);
        fx.load_mem(m, &at, ELEM_ALIGN)
    };
    fx.strb_conv(&sb, &conv, value);
    let inc = fx.w.bin("add", &i, &Value::i64(1));
    fx.w.store(&inc, &slot, 8, &[]);
    fx.w.br(head);
    fx.w.switch_to(done);
    fx.strb_lit(&sb, "]");
    fx.strb_finish(&sb);
    fx.finish();
}

fn struct_to_string(l: &mut Lcx<'_>, ty: TypeId, fn_name: &str) {
    let layout =
        l.cx.nstruct(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("to_string of unlaid struct {ty:?}"));
    let tuple = matches!(l.interner.kind(ty), TyKind::Tuple(_));
    let mut fx = glue(l, fn_name);
    let p = fx.arg(0);
    fx.ret_null_str_if_null(&p);
    let sb = fx.strb_new();
    let start = if tuple {
        "(".to_string()
    } else {
        format!("{} {{ ", layout.name)
    };
    fx.strb_lit(&sb, &start);
    for (i, f) in layout.fields.iter().enumerate() {
        let label = match (tuple, i) {
            (true, 0) => String::new(),
            (true, _) => ", ".into(),
            (false, 0) => format!("{}: ", f.name),
            (false, _) => format!(", {}: ", f.name),
        };
        if !label.is_empty() {
            fx.strb_lit(&sb, &label);
        }
        let v = fx.proto_field(&p, f);
        let conv = to_string_fn(&fx.l.cx, f.ty);
        fx.strb_conv(&sb, &conv, v);
    }
    fx.strb_lit(&sb, if tuple { ")" } else { " }" });
    fx.strb_finish(&sb);
    fx.finish();
}

fn union_variant_pieces(v: &dream_hir::UnionVariant) -> (String, Vec<String>, String) {
    if v.fields.is_empty() {
        return (v.name.clone(), Vec::new(), String::new());
    }
    let labels =
        v.fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let positional = f.name.strip_prefix('_').is_some_and(|rest| {
                    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
                });
                match (positional, i) {
                    (true, 0) => String::new(),
                    (true, _) => ", ".into(),
                    (false, 0) => format!("{}: ", f.name),
                    (false, _) => format!(", {}: ", f.name),
                }
            })
            .collect();
    (format!("{}(", v.name), labels, ")".into())
}

fn union_to_string(l: &mut Lcx<'_>, ty: TypeId, fn_name: &str) {
    let Some(layout) = l.cx.nunion(ty).cloned() else {
        let mut fx = glue(l, fn_name);
        fx.ret_str("<object>");
        fx.finish();
        return;
    };
    let niche = l.cx.niche_variant_discriminants(ty);
    let mut fx = glue(l, fn_name);
    let p = fx.arg(0);
    let sb = fx.strb_new();
    let d = match niche {
        Some((some, none)) => {
            let nz = fx.truthy(&p);
            V::s(fx.w.select(&nz, &Value::i32(some as i64), &Value::i32(none as i64)))
        }
        None => {
            fx.ret_null_str_if_null(&p);
            let pp = fx.ptr(&p);
            fx.load_ty(Ty::I32, &pp, 4, false)
        }
    };
    let join = fx.w.new_block("join");
    let mut arms = Vec::new();
    let mut bodies = Vec::new();
    for v in &layout.variants {
        if arms.iter().any(|(k, _)| *k == v.discriminant as i128) {
            continue;
        }
        let b = fx.w.new_block("variant");
        arms.push((v.discriminant as i128, b));
        bodies.push((b, v.clone()));
    }
    fx.w.switch(&d.v, join, &arms);
    for (b, v) in bodies {
        fx.w.switch_to(b);
        let (prefix, labels, suffix) = union_variant_pieces(&v);
        fx.strb_lit(&sb, &prefix);
        for (i, f) in v.fields.iter().enumerate() {
            fx.strb_lit(&sb, &labels[i]);
            let value = if niche.is_some() {
                p.clone()
            } else {
                fx.proto_field(&p, f)
            };
            let conv = to_string_fn(&fx.l.cx, f.ty);
            fx.strb_conv(&sb, &conv, value);
        }
        if !suffix.is_empty() {
            fx.strb_lit(&sb, &suffix);
        }
        fx.w.br(join);
    }
    fx.w.switch_to(join);
    fx.strb_finish(&sb);
    fx.finish();
}

impl<'l, 'a> Fx<'l, 'a> {
    /// `h = h * 31 + hash(field)` with C `int32_t` wrapping.
    fn hash_step(&mut self, h: &Value, p: &V, f: &FieldLayout) -> Value {
        let v = self.proto_field(p, f);
        let x = self.hash_code_of(f.ty, &v);
        let x = self.conv(&x, &Ty::I32);
        let m = self.w.bin("mul", h, &Value::i32(31));
        self.w.bin("add", &m, &x)
    }
}

fn struct_hash(l: &mut Lcx<'_>, ty: TypeId, fn_name: &str) {
    let layout =
        l.cx.nstruct(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("hash_code of unlaid struct {ty:?}"));
    let mut fx = glue(l, fn_name);
    let p = fx.arg(0);
    let mut h = Value::i32(17);
    for f in &layout.fields {
        h = fx.hash_step(&h, &p, f);
    }
    fx.w.ret(Some(&h));
    fx.finish();
}

fn union_hash(l: &mut Lcx<'_>, ty: TypeId, fn_name: &str) {
    let layout =
        l.cx.nunion(ty)
            .cloned()
            .unwrap_or_else(|| crate::internal_error!("hash_code of unlaid union {ty:?}"));
    let mut fx = glue(l, fn_name);
    let p = fx.arg(0);
    let nz = fx.truthy(&p);
    let d = fx.if_else_v(
        &nz,
        |fx| {
            let pp = fx.ptr(&p);
            fx.load_ty(Ty::I32, &pp, 4, false)
        },
        |_| V::i32(0),
    );
    let h0 = fx.w.bin("add", &Value::i32(17 * 31), &d.v);
    let slot = fx.w.alloca(Ty::I32, 4);
    fx.w.store(&h0, &slot, 4, &[]);
    let join = fx.w.new_block("join");
    let mut arms = Vec::new();
    let mut bodies = Vec::new();
    for v in &layout.variants {
        if v.fields.is_empty() || arms.iter().any(|(k, _)| *k == v.discriminant as i128) {
            continue;
        }
        let b = fx.w.new_block("variant");
        arms.push((v.discriminant as i128, b));
        bodies.push((b, v.clone()));
    }
    fx.w.switch(&d.v, join, &arms);
    for (b, v) in bodies {
        fx.w.switch_to(b);
        let mut h = fx.w.load(Ty::I32, &slot, 4, &[]);
        for f in &v.fields {
            h = fx.hash_step(&h, &p, f);
        }
        fx.w.store(&h, &slot, 4, &[]);
        fx.w.br(join);
    }
    fx.w.switch_to(join);
    let h = fx.w.load(Ty::I32, &slot, 4, &[]);
    fx.w.ret(Some(&h));
    fx.finish();
}

type RouterArm = Box<dyn Fn(&mut Fx<'_, '_>, &V) -> Value>;

/// `switch (dream_object_tag(p))` with one returning arm per case, after a null check that
/// returns `on_null`.
fn tag_router(
    l: &mut Lcx<'_>,
    name: &str,
    on_null: Value,
    arms: Vec<(Vec<i32>, RouterArm)>,
    default: RouterArm,
) {
    let mut fx = glue(l, name);
    let p = fx.arg(0);
    let z = fx.w.icmp("eq", &p.v, &Value::i64(0));
    fx.if_then(&z, |fx| fx.w.ret(Some(&on_null)));
    let tag = fx.call_v("dream_object_tag", std::slice::from_ref(&p));
    let tag = fx.conv(&tag, &Ty::I32);
    let other = fx.w.new_block("other");
    let mut cases: Vec<(i128, super::super::ir::BlockRef)> = Vec::new();
    let mut bodies = Vec::new();
    for (tags, f) in arms {
        let b = fx.w.new_block("tag");
        let mut used = false;
        for t in tags {
            if !cases.iter().any(|(k, _)| *k == t as i128) {
                cases.push((t as i128, b));
                used = true;
            }
        }
        if used {
            bodies.push((b, f));
        }
    }
    fx.w.switch(&tag, other, &cases);
    let ret_ty = fx.w.ret.clone();
    for (b, f) in bodies {
        fx.w.switch_to(b);
        let r = f(&mut fx, &p);
        let r = fx.conv(&V::s(r), &ret_ty);
        fx.w.ret(Some(&r));
    }
    fx.w.switch_to(other);
    let r = default(&mut fx, &p);
    let r = fx.conv(&V::s(r), &ret_ty);
    fx.w.ret(Some(&r));
    fx.finish();
}

type Arm = (Vec<i32>, Box<dyn Fn(&mut Fx<'_, '_>, &V) -> Value>);

fn const_arm(tag: i32, v: Value) -> Arm {
    (vec![tag], Box::new(move |_, _| v.clone()))
}

fn call_arm(tags: Vec<i32>, f: String, load: Option<Ty>) -> Arm {
    (
        tags,
        Box::new(move |fx, p| {
            let arg = match &load {
                Some(t) => {
                    let pp = fx.ptr(p);
                    fx.load_ty(t.clone(), &pp, 4, false)
                }
                None => p.clone(),
            };
            fx.call_v(&f, &[arg]).v
        }),
    )
}

fn tagged_arms(l: &Lcx<'_>, suffix: &str, skip_tuples: bool) -> Vec<Arm> {
    let mut tagged: Vec<(TypeId, i32)> = l.cx.tags.iter().map(|(t, g)| (*t, *g)).collect();
    tagged.sort_by_key(|(_, t)| *t);
    let mut out = Vec::new();
    for (ty, tag) in tagged {
        let name = if let Some(s) = l.cx.native.structs.get(&ty) {
            if skip_tuples && matches!(l.interner.kind(ty), TyKind::Tuple(_)) {
                continue;
            }
            &s.name
        } else if let Some(u) = l.cx.native.unions.get(&ty) {
            &u.name
        } else {
            continue;
        };
        out.push(call_arm(
            vec![tag],
            c_ident(&format!("{name}{suffix}")),
            None,
        ));
    }
    out
}

fn type_name_router(l: &mut Lcx<'_>) {
    let mut arms: Vec<Arm> = BUILTIN_TYPE_NAMES
        .iter()
        .map(|&(tag, name)| const_arm(builtin_tag(tag), l.str_val(name)))
        .collect();
    let mut tagged: Vec<(TypeId, i32)> = l.cx.tags.iter().map(|(t, g)| (*t, *g)).collect();
    tagged.sort_by_key(|(_, t)| *t);
    for (ty, tag) in tagged {
        if let Some(name) = l.mir.type_names.get(&ty) {
            arms.push(const_arm(tag, l.str_val(name)));
        }
    }
    let null = l.str_val(NULL_TYPE_NAME);
    let unknown = l.str_val(UNKNOWN_TYPE_NAME);
    tag_router(
        l,
        "dream_object_type_name",
        null,
        arms,
        Box::new(move |_, _| unknown.clone()),
    );
}

fn to_string_router(l: &mut Lcx<'_>) {
    let i32t = Some(Ty::I32);
    let mut arms: Vec<Arm> = vec![
        call_arm(
            vec![abi::TAG_INT],
            "dream_int_to_string".into(),
            i32t.clone(),
        ),
        call_arm(
            vec![abi::TAG_UINT],
            "dream_uint_to_string".into(),
            i32t.clone(),
        ),
        call_arm(
            vec![abi::TAG_LONG],
            "dream_long_to_string".into(),
            Some(Ty::I64),
        ),
        call_arm(
            vec![abi::TAG_ULONG],
            "dream_ulong_to_string".into(),
            Some(Ty::I64),
        ),
        call_arm(
            vec![abi::TAG_BYTE],
            "dream_byte_to_string".into(),
            i32t.clone(),
        ),
        call_arm(
            vec![abi::TAG_BOOL],
            "dream_bool_to_string".into(),
            i32t.clone(),
        ),
        call_arm(vec![abi::TAG_CHAR], "dream_char_to_string".into(), i32t),
        call_arm(
            vec![abi::TAG_FLOAT],
            "dream_float_to_string".into(),
            Some(Ty::F32),
        ),
        call_arm(
            vec![abi::TAG_DOUBLE],
            "dream_double_to_string".into(),
            Some(Ty::F64),
        ),
        (
            vec![abi::TAG_STRING],
            Box::new(|fx, p| {
                fx.call("dream_retain", std::slice::from_ref(p));
                p.v.clone()
            }),
        ),
        call_arm(vec![abi::TAG_ARRAY], "dream_array_to_string".into(), None),
    ];
    arms.extend(tagged_arms(l, "_to_string", true));
    let null = l.str_val("null");
    let obj = l.str_val("<object>");
    tag_router(
        l,
        "dream_object_to_string",
        null,
        arms,
        Box::new(move |_, _| obj.clone()),
    );
}

fn hash_code_router(l: &mut Lcx<'_>) {
    let mut arms: Vec<Arm> = vec![
        (
            vec![
                abi::TAG_INT,
                abi::TAG_UINT,
                abi::TAG_BOOL,
                abi::TAG_CHAR,
                abi::TAG_BYTE,
            ],
            Box::new(|fx, p| {
                let pp = fx.ptr(p);
                fx.load_ty(Ty::I32, &pp, 4, false).v
            }),
        ),
        call_arm(
            vec![abi::TAG_LONG, abi::TAG_ULONG],
            "dream_hash_long".into(),
            Some(Ty::I64),
        ),
        call_arm(
            vec![abi::TAG_FLOAT],
            "dream_bitcast_f32".into(),
            Some(Ty::F32),
        ),
        call_arm(
            vec![abi::TAG_DOUBLE],
            "dream_hash_double".into(),
            Some(Ty::F64),
        ),
        call_arm(vec![abi::TAG_STRING], "dream_string_hash".into(), None),
    ];
    arms.extend(tagged_arms(l, "_hash_code", true));
    tag_router(
        l,
        "dream_object_hash_code",
        Value::i32(0),
        arms,
        Box::new(|fx, p| fx.conv(&V::u(p.v.clone()), &Ty::I32)),
    );
}
