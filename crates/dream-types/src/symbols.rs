//! Structural symbol components, independent of interner allocation order.

use crate::{DefTable, TyKind, TypeId, TypeInterner};
use std::fmt::Write;

/// Length framing and byte escaping keep punctuation, underscores, and nested arguments distinct.
pub fn symbol_component(name: &str) -> String {
    let mut out = format!("{}", name.len());
    out.push('_');
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() {
            out.push(byte as char);
        } else {
            write!(&mut out, "_{byte:02x}").expect("writing to a String cannot fail");
        }
    }
    out
}

pub fn type_symbol(interner: &TypeInterner, defs: &DefTable, ty: TypeId) -> String {
    fn list(interner: &TypeInterner, defs: &DefTable, args: &[TypeId]) -> String {
        let mut out = format!("{}", args.len());
        for &arg in args {
            out.push('_');
            out.push_str(&symbol_component(&type_symbol(interner, defs, arg)));
        }
        out
    }
    match interner.kind(ty) {
        TyKind::Prim(p) => format!("p{}", symbol_component(p.name())),
        TyKind::Object => "object".into(),
        TyKind::Void => "void".into(),
        TyKind::Error => "error".into(),
        TyKind::Js => "js".into(),
        TyKind::Array(elem) => format!("a{}", list(interner, defs, &[*elem])),
        TyKind::Tuple(elems) => format!("t{}", list(interner, defs, elems)),
        TyKind::Func(params, ret) => format!(
            "f{}_r{}",
            list(interner, defs, params),
            symbol_component(&type_symbol(interner, defs, *ret))
        ),
        TyKind::Struct(def, args) | TyKind::Union(def, args) | TyKind::Interface(def, args) => {
            let tag = match interner.kind(ty) {
                TyKind::Struct(..) => 's',
                TyKind::Union(..) => 'u',
                _ => 'i',
            };
            format!(
                "{tag}{}_{}",
                symbol_component(defs.name(*def)),
                list(interner, defs, args)
            )
        }
        TyKind::Enum(def) => format!("e{}", symbol_component(defs.name(*def))),
    }
}

/// `_D` is an escaped namespace: user spellings beginning with it take the distinct `e` branch.
pub fn function_symbol(module: Option<&str>, name: &str, args: &[String]) -> String {
    if module.is_none() && args.is_empty() {
        return if name.starts_with("_D")
            || name.contains("__")
            || name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && b != b'_')
        {
            format!("_De{}", symbol_component(name))
        } else {
            name.to_string()
        };
    }
    let mut out = format!(
        "_Dg{}{}",
        symbol_component(module.unwrap_or("")),
        symbol_component(name)
    );
    for arg in args {
        out.push('_');
        out.push_str(&symbol_component(arg));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefKind, TypeCtx};

    #[test]
    fn symbols_do_not_depend_on_type_or_definition_allocation_order() {
        let mut a = TypeCtx::new();
        let mut b = TypeCtx::new();
        b.register(DefKind::Struct, "Unrelated", vec![]);
        b.interner.array(b.interner.string());
        let ad = a.register(DefKind::Struct, "Box", vec!["T".into()]);
        let bd = b.register(DefKind::Struct, "Box", vec!["T".into()]);
        let at = a.interner.struct_ty(ad, vec![a.interner.int()]);
        let bt = b.interner.struct_ty(bd, vec![b.interner.int()]);
        assert_ne!(at, bt);
        let asy = type_symbol(&a.interner, &a.defs, at);
        let bsy = type_symbol(&b.interner, &b.defs, bt);
        assert_eq!(asy, bsy);
        assert_eq!(
            function_symbol(Some("users"), "id", &[asy]),
            function_symbol(Some("users"), "id", &[bsy])
        );
    }

    #[test]
    fn framing_prevents_identifier_and_argument_collisions() {
        assert_ne!(symbol_component("a_b"), symbol_component("a.b"));
        assert_ne!(
            function_symbol(None, "foo__12", &[]),
            function_symbol(None, "foo", &["12".into()])
        );
        assert_ne!(
            function_symbol(None, "_Dg0_3_foo_2_12", &[]),
            function_symbol(None, "foo", &["12".into()])
        );
        assert_ne!(
            function_symbol(None, "foo", &["a".into(), "b".into()]),
            function_symbol(None, "foo", &["a_b".into()])
        );
        assert_ne!(
            function_symbol(Some("a.b"), "foo", &[]),
            function_symbol(Some("a_b"), "foo", &[])
        );
    }
}
