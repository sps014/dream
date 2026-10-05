use dream_syntax::nodes::Type;
use std::collections::BTreeSet;
use std::collections::HashSet;

#[derive(Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub(super) struct CollectionSpec {
    pub(super) kind: String,
    pub(super) elem_type: String,
    pub(super) value_type: String,
    pub(super) self_ty: String,
    pub(super) fn_suffix: String,
}

pub(super) fn collection_fn_suffix(mangled: &str) -> String {
    mangled.replace("[]", "__arr")
}

pub(super) fn json_elem_supported(name: &str, jsonable: &HashSet<String>) -> bool {
    matches!(
        name,
        "int" | "long" | "string" | "bool" | "double" | "float" | "JsonValue"
    ) || jsonable.contains(name)
}

pub(super) fn insert_collection_spec(
    out: &mut BTreeSet<CollectionSpec>,
    kind: &str,
    elem_type: String,
    value_type: String,
    ty: &Type,
    jsonable: &HashSet<String>,
) {
    let elem_ok = if kind == "map" || kind == "sortedmap" {
        json_elem_supported(&value_type, jsonable)
    } else {
        json_elem_supported(&elem_type, jsonable)
    };
    if !elem_ok {
        return;
    }
    let self_ty = ty.display_name();
    let mangled = ty.get_type();
    out.insert(CollectionSpec {
        kind: kind.to_string(),
        elem_type,
        value_type,
        self_ty,
        fn_suffix: collection_fn_suffix(&mangled),
    });
}

pub(super) fn collect_collections_from_type(
    ty: &Type,
    jsonable: &HashSet<String>,
    out: &mut BTreeSet<CollectionSpec>,
) {
    match ty {
        Type::Array(inner) => {
            insert_collection_spec(out, "array", inner.get_type(), String::new(), ty, jsonable);
            collect_collections_from_type(inner, jsonable, out);
        }
        Type::Struct(token, Some(args)) => match token.text.as_str() {
            "List" if args.len() == 1 => {
                insert_collection_spec(
                    out,
                    "list",
                    args[0].get_type(),
                    String::new(),
                    ty,
                    jsonable,
                );
                collect_collections_from_type(&args[0], jsonable, out);
            }
            "Set" if args.len() == 1 => {
                insert_collection_spec(out, "set", args[0].get_type(), String::new(), ty, jsonable);
                collect_collections_from_type(&args[0], jsonable, out);
            }
            "Map" if args.len() == 2 && args[0].get_type() == "string" => {
                insert_collection_spec(out, "map", String::new(), args[1].get_type(), ty, jsonable);
                collect_collections_from_type(&args[1], jsonable, out);
            }
            "SortedMap" if args.len() == 2 && args[0].get_type() == "string" => {
                insert_collection_spec(
                    out,
                    "sortedmap",
                    String::new(),
                    args[1].get_type(),
                    ty,
                    jsonable,
                );
                collect_collections_from_type(&args[1], jsonable, out);
            }
            "Option" if args.len() == 1 => {
                collect_collections_from_type(&args[0], jsonable, out);
            }
            _ => {
                for arg in args {
                    collect_collections_from_type(arg, jsonable, out);
                }
            }
        },
        Type::Tuple(elems) => {
            for elem in elems {
                collect_collections_from_type(elem, jsonable, out);
            }
        }
        _ => {}
    }
}
