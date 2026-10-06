//! Syntax `Type` → snapshot [`TypeRef`] (resolved declaration identity + structured args).

use super::decls::DeclIndex;
use super::model::TypeRef;
use dream_syntax::nodes::Type;

pub(super) fn type_ref(ty: &Type, index: &DeclIndex, generics: &[String]) -> TypeRef {
    let (kind, name, decl, args) = match ty {
        Type::Void => ("void", "void".to_string(), String::new(), Vec::new()),
        Type::Unknown | Type::GenericFunctionItem(_) => {
            ("unknown", String::new(), String::new(), Vec::new())
        }
        Type::Generic(name) => ("param", name.clone(), String::new(), Vec::new()),
        Type::Array(inner) => (
            "array",
            String::new(),
            String::new(),
            vec![type_ref(inner, index, generics)],
        ),
        Type::Tuple(elems) => (
            "tuple",
            String::new(),
            String::new(),
            elems.iter().map(|e| type_ref(e, index, generics)).collect(),
        ),
        Type::Function(params, ret) => {
            let mut args: Vec<TypeRef> = params
                .iter()
                .map(|p| type_ref(p, index, generics))
                .collect();
            args.push(type_ref(ret, index, generics));
            ("function", String::new(), String::new(), args)
        }
        Type::Struct(tok, generic_args) => {
            let args = generic_args
                .iter()
                .flatten()
                .map(|a| type_ref(a, index, generics))
                .collect();
            if generic_args.is_none() && generics.iter().any(|g| g == &tok.text) {
                ("param", tok.text.clone(), String::new(), Vec::new())
            } else {
                let decl = index
                    .type_decl(&tok.text)
                    .map(|e| e.id.clone())
                    .unwrap_or_default();
                ("named", tok.text.clone(), decl, args)
            }
        }
        prim => ("prim", prim.get_type(), String::new(), Vec::new()),
    };
    TypeRef {
        kind: kind.to_string(),
        name,
        display: ty.display_name(),
        mangled: ty.get_type(),
        decl,
        args,
    }
}

pub(super) fn unknown_type() -> TypeRef {
    TypeRef {
        kind: "unknown".to_string(),
        ..TypeRef::default()
    }
}
