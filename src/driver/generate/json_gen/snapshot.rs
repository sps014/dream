use super::super::quote::json_string;
use super::collection_discovery::collect_all_collections;
use super::collection_types::CollectionSpec;
use crate::driver::source_loader::ProgramAccumulator;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::EnumDeclarationNode;
use dream_syntax::nodes::Type;
use std::collections::HashSet;

pub(super) fn build_snapshot(
    acc: &ProgramAccumulator<'_>,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
    json_names: &HashSet<String>,
    jsonable: &HashSet<String>,
) -> String {
    let mut types = String::from("[");
    let mut first = true;
    for s in structs
        .iter()
        .filter(|s| s.attributes.iter().any(|a| a.name.text == "json"))
    {
        if !first {
            types.push(',');
        }
        first = false;
        types.push_str(&snapshot_class(s));
    }
    for e in enums
        .iter()
        .filter(|e| e.attributes.iter().any(|a| a.name.text == "json") && e.is_data_enum())
    {
        if !first {
            types.push(',');
        }
        first = false;
        types.push_str(&snapshot_union(e));
    }
    types.push(']');

    let collections = collect_all_collections(acc, jsonable);
    let mut col_json = String::from("[");
    for (i, c) in collections.iter().enumerate() {
        if i > 0 {
            col_json.push(',');
        }
        col_json.push_str(&snapshot_collection(c));
    }
    col_json.push(']');

    format!(
        "{{\"types\":{},\"json_names\":{},\"jsonable\":{},\"collections\":{}}}",
        types,
        json_string_array(json_names.iter().cloned().collect()),
        json_string_array(jsonable.iter().cloned().collect()),
        col_json,
    )
}

pub(super) fn snapshot_class(s: &StructDeclarationNode<'_>) -> String {
    let generic_params: Vec<String> = s
        .generic_parameters
        .as_ref()
        .map(|ps| ps.iter().map(|p| p.text.clone()).collect())
        .unwrap_or_default();
    let mut fields = String::from("[");
    let mut first = true;
    for field in &s.fields {
        if !first {
            fields.push(',');
        }
        first = false;
        fields.push_str(&snapshot_field(
            &field.name.text,
            &field.field_type.display_name(),
            &field.field_type,
            &field.attributes,
            &generic_params,
        ));
    }
    fields.push(']');
    format!(
        "{{\"name\":{},\"is_union\":false,\"generic_params\":{},\"fields\":{},\"variants\":[]}}",
        json_string(&s.name.text),
        json_string_array(generic_params),
        fields,
    )
}

pub(super) fn snapshot_union(e: &EnumDeclarationNode<'_>) -> String {
    let generic_params: Vec<String> = e
        .generic_parameters
        .as_ref()
        .map(|ps| ps.iter().map(|p| p.text.clone()).collect())
        .unwrap_or_default();
    let mut variants = String::from("[");
    let mut first_v = true;
    for variant in &e.variants {
        if !first_v {
            variants.push(',');
        }
        first_v = false;
        let mut fields = String::from("[");
        let mut first_f = true;
        for field in &variant.fields {
            if !first_f {
                fields.push(',');
            }
            first_f = false;
            fields.push_str(&snapshot_field(
                &field.name.text,
                &field.field_type.display_name(),
                &field.field_type,
                &field.attributes,
                &generic_params,
            ));
        }
        fields.push(']');
        variants.push_str(&format!(
            "{{\"name\":{},\"fields\":{}}}",
            json_string(&variant.name.text),
            fields
        ));
    }
    variants.push(']');
    format!(
        "{{\"name\":{},\"is_union\":true,\"generic_params\":{},\"fields\":[],\"variants\":{}}}",
        json_string(&e.name.text),
        json_string_array(generic_params),
        variants,
    )
}

pub(super) fn snapshot_field(
    name: &str,
    type_name: &str,
    field_ty: &Type,
    attrs: &[dream_syntax::nodes::AttributeNode],
    generic_params: &[String],
) -> String {
    let json_ignore = attrs.iter().any(|a| a.name.text == "json_ignore");
    let mut property_name = String::new();
    if let Some(prop) = attrs.iter().find(|a| a.name.text == "property_name") {
        if let Some(arg) = prop.args.first() {
            property_name = arg.as_string().unwrap_or("").to_string();
        }
    }
    let option_inner = match field_ty {
        Type::Struct(token, Some(args)) if token.text == "Option" && args.len() == 1 => {
            args[0].get_type()
        }
        _ => String::new(),
    };
    // `Map<string, V>` / `SortedMap<string, V>` fields widen `@json` support; the key type must be
    // `string` since JSON object keys are strings.
    let (map_value_inner, map_ctor) = match field_ty {
        Type::Struct(token, Some(args))
            if (token.text == "Map" || token.text == "SortedMap")
                && args.len() == 2
                && args[0].get_type() == "string" =>
        {
            (args[1].get_type(), token.text.clone())
        }
        _ => (String::new(), String::new()),
    };
    let (seq_elem_inner, seq_kind) = match field_ty {
        Type::Struct(token, Some(args)) if token.text == "List" && args.len() == 1 => {
            (args[0].get_type(), "list".to_string())
        }
        Type::Struct(token, Some(args)) if token.text == "Set" && args.len() == 1 => {
            (args[0].get_type(), "set".to_string())
        }
        _ => (String::new(), String::new()),
    };
    let is_type_param = generic_params.iter().any(|p| p == type_name);
    format!(
        "{{\"name\":{},\"type_name\":{},\"json_ignore\":{},\"property_name\":{},\"option_inner\":{},\"is_type_param\":{},\"map_value_inner\":{},\"map_ctor\":{},\"seq_elem_inner\":{},\"seq_kind\":{}}}",
        json_string(name),
        json_string(type_name),
        if json_ignore { "true" } else { "false" },
        json_string(&property_name),
        json_string(&option_inner),
        if is_type_param { "true" } else { "false" },
        json_string(&map_value_inner),
        json_string(&map_ctor),
        json_string(&seq_elem_inner),
        json_string(&seq_kind),
    )
}

pub(super) fn json_string_array(mut items: Vec<String>) -> String {
    // Deterministic order for reproducible harness input (not required for correctness).
    items.sort();
    let mut out = String::from("[");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_string(s));
    }
    out.push(']');
    out
}

pub(super) fn snapshot_collection(c: &CollectionSpec) -> String {
    format!(
        "{{\"kind\":{},\"elem_type\":{},\"value_type\":{},\"self_ty\":{},\"fn_suffix\":{}}}",
        json_string(&c.kind),
        json_string(&c.elem_type),
        json_string(&c.value_type),
        json_string(&c.self_ty),
        json_string(&c.fn_suffix),
    )
}
