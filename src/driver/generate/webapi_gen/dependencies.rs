use super::attributes::attr_enum_name;
use super::attributes::attr_string;
use super::attributes::has_attr;
use super::types::option_inner;
use super::types::result_parts;
use super::types::type_name;
use crate::driver::source_loader::ProgramAccumulator;
use std::collections::HashSet;

pub(super) fn emit_dep_call(
    s: &mut String,
    ind: &str,
    bind: &str,
    ty: &str,
    dep: &str,
    acc: &ProgramAccumulator<'_>,
    memo: &mut HashSet<String>,
) {
    let slot = format!("__dep_{dep}");
    if memo.contains(dep) {
        s.push_str(&format!("{ind}let {bind} = {slot};\n"));
        return;
    }
    memo.insert(dep.to_string());
    let dep_fn = acc.all_functions.iter().find(|f| f.name.text == dep);
    let mut dep_args: Vec<String> = Vec::new();
    if let Some(df) = dep_fn {
        for (j, p) in df.parameters.iter().enumerate() {
            let pty = type_name(&p.type_);
            let tmp = format!("__d_{bind}_{j}");
            if let Some(nested) = attr_enum_name(&p.attributes, "dep") {
                emit_dep_call(s, ind, &tmp, &pty, &nested, acc, memo);
                dep_args.push(tmp);
            } else if has_attr(&p.attributes, "header") {
                let h = attr_string(&p.attributes, "header").unwrap_or_else(|| p.name.text.clone());
                s.push_str(&format!(
                    "{ind}let {tmp}_o = ctx.incoming.header(\"{h}\");\n"
                ));
                if option_inner(&pty).is_some() {
                    s.push_str(&format!("{ind}let {tmp} = {tmp}_o;\n"));
                } else {
                    s.push_str(&format!(
                        "{ind}if {tmp}_o.is_none() {{ return HttpOutgoing.from_status(HttpStatus.unauthorized()); }}\n"
                    ));
                    s.push_str(&format!(
                        "{ind}let {tmp} = {tmp}_o.unwrap_or(string.empty);\n"
                    ));
                }
                dep_args.push(tmp);
            } else if has_attr(&p.attributes, "query") {
                let q = attr_string(&p.attributes, "query").unwrap_or_else(|| p.name.text.clone());
                s.push_str(&format!(
                    "{ind}let {tmp} = ctx.incoming.query_param(\"{q}\").unwrap_or(string.empty);\n"
                ));
                dep_args.push(tmp);
            } else if pty == "HttpIncoming" {
                dep_args.push("ctx.incoming".into());
            } else if pty == "RequestContext" {
                s.push_str(&format!("{ind}let {tmp} = ctx;\n"));
                dep_args.push(tmp);
            } else {
                s.push_str(&format!(
                    "{ind}return HttpOutgoing.from_status(HttpStatus(500, \"unsupported dependency parameter\"));\n"
                ));
                return;
            }
        }
    }
    let args = dep_args.join(", ");
    let call = if dep_fn.map(|f| f.is_async).unwrap_or(true) {
        format!("{dep}({args}).await")
    } else {
        format!("{dep}({args})")
    };
    if let Some((_, err)) = result_parts(ty) {
        if err == "HttpStatus" {
            s.push_str(&format!("{ind}let __dr_{bind} = {call};\n"));
            s.push_str(&format!(
                "{ind}if __dr_{bind}.is_err() {{ return HttpOutgoing.from_status(__dr_{bind}.unwrap_err()); }}\n"
            ));
            s.push_str(&format!("{ind}let {slot} = __dr_{bind}.unwrap();\n"));
            s.push_str(&format!("{ind}let {bind} = {slot};\n"));
            return;
        }
    }
    let dep_ret = dep_fn
        .and_then(|f| f.return_type.as_ref())
        .map(type_name)
        .unwrap_or_else(|| ty.to_string());
    if let Some((ok, err)) = result_parts(&dep_ret) {
        if err == "HttpStatus" {
            s.push_str(&format!("{ind}let __dr_{bind} = {call};\n"));
            s.push_str(&format!(
                "{ind}if __dr_{bind}.is_err() {{ return HttpOutgoing.from_status(__dr_{bind}.unwrap_err()); }}\n"
            ));
            s.push_str(&format!("{ind}let {slot} = __dr_{bind}.unwrap();\n"));
            s.push_str(&format!("{ind}let {bind} = {slot};\n"));
            let _ = ok;
            return;
        }
    }
    s.push_str(&format!("{ind}let {slot} = {call};\n"));
    s.push_str(&format!("{ind}let {bind} = {slot};\n"));
}
