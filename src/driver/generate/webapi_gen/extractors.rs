use super::dependencies::emit_dep_call;
use super::emit::emit_return;
use super::model::ParamKind;
use super::model::Route;
use super::types::option_inner;
use crate::driver::source_loader::ProgramAccumulator;
use std::collections::HashSet;

pub(super) fn emit_extractors(
    s: &mut String,
    r: &Route,
    i: usize,
    acc: &ProgramAccumulator<'_>,
    ind: &str,
) {
    let mut args: Vec<String> = Vec::new();
    let mut dep_memo: HashSet<String> = HashSet::new();
    let needs_multipart = r
        .params
        .iter()
        .any(|p| matches!(p.kind, ParamKind::Form(_) | ParamKind::File(_)));
    if needs_multipart {
        s.push_str(&format!(
            "{ind}if WebApp.host_parse_multipart(ctx.incoming.req_id) != 1 {{ return HttpOutgoing.from_status(HttpStatus(400, \"expected multipart/form-data\")); }}\n"
        ));
    }
    for p in &r.params {
        match &p.kind {
            ParamKind::Incoming => {
                s.push_str(&format!("{ind}let {} = ctx.incoming;\n", p.name));
                args.push(p.name.clone());
            }
            ParamKind::Context => {
                s.push_str(&format!("{ind}let {} = ctx;\n", p.name));
                args.push(p.name.clone());
            }
            ParamKind::ServerWs => {
                s.push_str(&format!(
                    "{ind}let __ws_{} = ServerWebSocket.upgrade(ctx.incoming);\n",
                    p.name
                ));
                s.push_str(&format!(
                    "{ind}if __ws_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400, \"websocket upgrade failed\")); }}\n",
                    p.name
                ));
                s.push_str(&format!(
                    "{ind}let {} = __ws_{}.unwrap();\n",
                    p.name, p.name
                ));
                args.push(p.name.clone());
            }
            ParamKind::Path(key) => {
                s.push_str(&format!(
                    "{ind}let __p_{} = __params{i}.get(\"{key}\").unwrap_or(string.empty);\n",
                    p.name
                ));
                emit_parse_scalar(s, ind, &p.name, &p.ty, &format!("__p_{}", p.name));
                args.push(p.name.clone());
            }
            ParamKind::Query(key) => {
                s.push_str(&format!(
                    "{ind}let __q_{} = ctx.incoming.query_param(\"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __q_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __q_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing query {key}\")); }}\n",
                        p.name
                    ));
                    emit_parse_scalar(
                        s,
                        ind,
                        &p.name,
                        &p.ty,
                        &format!("__q_{}.unwrap_or(string.empty)", p.name),
                    );
                }
                args.push(p.name.clone());
            }
            ParamKind::Header(key) => {
                s.push_str(&format!(
                    "{ind}let __h_{} = ctx.incoming.header(\"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __h_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __h_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing header {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __h_{}.unwrap_or(string.empty);\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Cookie(key) => {
                s.push_str(&format!(
                    "{ind}let __c_{} = ctx.incoming.cookie(\"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __c_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __c_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing cookie {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __c_{}.unwrap_or(string.empty);\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Body => {
                s.push_str(&format!(
                    "{ind}let __body_{} = ctx.incoming.read_body_text();\n",
                    p.name
                ));
                if p.ty == "string" {
                    s.push_str(&format!("{ind}let {} = __body_{};\n", p.name, p.name));
                } else if p.ty == "byte[]" {
                    s.push_str(&format!(
                        "{ind}let {} = ctx.incoming.read_body_bytes();\n",
                        p.name
                    ));
                } else {
                    s.push_str(&format!(
                        "{ind}let __bj_{} = Json.deserialize<{}>(__body_{});\n",
                        p.name, p.ty, p.name
                    ));
                    s.push_str(&format!(
                        "{ind}if __bj_{}.is_err() {{ return HttpOutgoing.from_status(HttpStatus(400,\"invalid json\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __bj_{}.unwrap();\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Form(key) => {
                s.push_str(&format!(
                    "{ind}let __f_{} = WebApp.multipart_field(ctx.incoming.req_id, \"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __f_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __f_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing form {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __f_{}.unwrap_or(string.empty);\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::File(key) => {
                s.push_str(&format!(
                    "{ind}let __file_{} = WebApp.multipart_file(ctx.incoming.req_id, \"{key}\");\n",
                    p.name
                ));
                if option_inner(&p.ty).is_some() {
                    s.push_str(&format!("{ind}let {} = __file_{};\n", p.name, p.name));
                } else {
                    s.push_str(&format!(
                        "{ind}if __file_{}.is_none() {{ return HttpOutgoing.from_status(HttpStatus(400,\"missing file {key}\")); }}\n",
                        p.name
                    ));
                    s.push_str(&format!(
                        "{ind}let {} = __file_{}.unwrap();\n",
                        p.name, p.name
                    ));
                }
                args.push(p.name.clone());
            }
            ParamKind::Dep(dep) => {
                emit_dep_call(s, ind, &p.name, &p.ty, dep, acc, &mut dep_memo);
                args.push(p.name.clone());
            }
        }
    }
    let call_args = args.join(", ");
    let call = if r.is_async {
        format!("{}({call_args}).await", r.fn_name)
    } else {
        format!("{}({call_args})", r.fn_name)
    };
    if r.websocket {
        s.push_str(&format!("{ind}{call};\n"));
        s.push_str(&format!("{ind}return HttpOutgoing.already_sent();\n"));
        return;
    }
    emit_return(s, ind, &r.ret, &call);
}

pub(super) fn emit_parse_scalar(s: &mut String, ind: &str, name: &str, ty: &str, src: &str) {
    match ty {
        "string" => s.push_str(&format!("{ind}let {name} = {src};\n")),
        "int" => {
            s.push_str(&format!("{ind}let __pi_{name} = int.parse({src});\n"));
            s.push_str(&format!(
                "{ind}if __pi_{name}.is_err() {{ return HttpOutgoing.from_status(HttpStatus(400,\"invalid {name}\")); }}\n"
            ));
            s.push_str(&format!("{ind}let {name} = __pi_{name}.unwrap();\n"));
        }
        "bool" => {
            s.push_str(&format!(
                "{ind}let {name} = {src} == \"true\" || {src} == \"1\";\n"
            ));
        }
        _ => s.push_str(&format!("{ind}let {name} = {src};\n")),
    }
}
