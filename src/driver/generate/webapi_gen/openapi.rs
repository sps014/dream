use super::super::quote::json_string;
use super::model::ParamKind;
use super::model::Route;
use super::types::option_inner;
use super::types::type_name;
use crate::driver::source_loader::ProgramAccumulator;
use indexmap::IndexMap;
use std::collections::HashSet;

pub(super) fn openapi_paths(
    routes: &[Route],
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) -> String {
    let mut s = String::from("{");
    let mut first_path = true;
    let mut by_path: IndexMap<String, Vec<&Route>> = IndexMap::new();
    for r in routes {
        by_path.entry(r.path.clone()).or_default().push(r);
    }
    for (path, rs) in &by_path {
        if !first_path {
            s.push(',');
        }
        first_path = false;
        s.push_str(&json_string(path));
        s.push_str(":{");
        let mut first_m = true;
        for r in rs {
            if !first_m {
                s.push(',');
            }
            first_m = false;
            s.push_str(&json_string(&r.method.to_lowercase()));
            s.push_str(":{\"operationId\":");
            s.push_str(&json_string(&r.fn_name));
            let mut params_json = String::new();
            let mut first_p = true;
            params_json.push('[');
            let mut body: Option<String> = None;
            let mut form_props = String::new();
            let mut first_form = true;
            for p in &r.params {
                match &p.kind {
                    ParamKind::Path(n) => {
                        if !first_p {
                            params_json.push(',');
                        }
                        first_p = false;
                        params_json.push_str("{\"name\":");
                        params_json.push_str(&json_string(n));
                        params_json
                            .push_str(",\"in\":\"path\",\"required\":true,\"schema\":{\"type\":");
                        params_json.push_str(&json_string(openapi_scalar(&p.ty)));
                        params_json.push_str("}}");
                    }
                    ParamKind::Query(n) | ParamKind::Header(n) => {
                        if !first_p {
                            params_json.push(',');
                        }
                        first_p = false;
                        let loc = if matches!(&p.kind, ParamKind::Query(_)) {
                            "query"
                        } else {
                            "header"
                        };
                        params_json.push_str("{\"name\":");
                        params_json.push_str(&json_string(n));
                        params_json.push_str(",\"in\":");
                        params_json.push_str(&json_string(loc));
                        params_json.push_str(",\"required\":");
                        params_json.push_str(if option_inner(&p.ty).is_some() {
                            "false"
                        } else {
                            "true"
                        });
                        params_json.push_str(",\"schema\":{\"type\":\"string\"}}");
                    }
                    ParamKind::Body => {
                        body = Some(openapi_schema_str(&p.ty, json_names, acc));
                    }
                    ParamKind::Form(n) => {
                        if !first_form {
                            form_props.push(',');
                        }
                        first_form = false;
                        form_props.push_str(&json_string(n));
                        form_props.push_str(":{\"type\":\"string\"}");
                    }
                    ParamKind::File(n) => {
                        if !first_form {
                            form_props.push(',');
                        }
                        first_form = false;
                        form_props.push_str(&json_string(n));
                        form_props.push_str(":{\"type\":\"string\",\"format\":\"binary\"}");
                    }
                    _ => {}
                }
            }
            params_json.push(']');
            if params_json != "[]" {
                s.push_str(",\"parameters\":");
                s.push_str(&params_json);
            }
            if let Some(schema) = body {
                s.push_str(",\"requestBody\":{\"required\":true,\"content\":{\"application/json\":{\"schema\":");
                s.push_str(&schema);
                s.push_str("}}}");
            } else if !form_props.is_empty() {
                s.push_str(",\"requestBody\":{\"required\":true,\"content\":{\"multipart/form-data\":{\"schema\":{\"type\":\"object\",\"properties\":{");
                s.push_str(&form_props);
                s.push_str("}}}}}");
            }
            s.push_str(",\"responses\":{\"200\":{\"description\":\"OK\"}}}");
        }
        s.push('}');
    }
    s.push('}');
    s
}

pub(super) fn openapi_schema_str(
    ty: &str,
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) -> String {
    if json_names.contains(ty) {
        if let Some(st) = acc.all_structs.iter().find(|s| s.name.text == ty) {
            let mut s = String::from("{\"type\":\"object\",\"properties\":{");
            let mut first = true;
            let mut req = String::from("[");
            let mut first_r = true;
            for f in &st.fields {
                if !first {
                    s.push(',');
                }
                first = false;
                s.push_str(&json_string(&f.name.text));
                s.push_str(":{\"type\":");
                s.push_str(&json_string(openapi_scalar(&type_name(&f.field_type))));
                s.push('}');
                if !first_r {
                    req.push(',');
                }
                first_r = false;
                req.push_str(&json_string(&f.name.text));
            }
            req.push(']');
            s.push_str("},\"required\":");
            s.push_str(&req);
            s.push('}');
            return s;
        }
    }
    "{\"type\":\"object\"}".to_string()
}

pub(super) fn openapi_scalar(ty: &str) -> &'static str {
    match ty {
        "int" | "uint" | "long" | "ulong" | "byte" => "integer",
        "float" | "double" => "number",
        "bool" => "boolean",
        _ => "string",
    }
}
