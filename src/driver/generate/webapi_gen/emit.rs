use super::super::quote::dream_string;
use super::extractors::emit_extractors;
use super::model::Route;
use super::openapi::openapi_paths;
use super::types::result_parts;
use crate::driver::source_loader::ProgramAccumulator;
use std::collections::HashSet;

pub(super) fn stub_extend() -> String {
    emit_extend(&[], &[], &HashSet::new(), &ProgramAccumulator::default())
}

pub(super) fn emit_extend(
    routes: &[Route],
    middleware: &[(i32, String)],
    json_names: &HashSet<String>,
    acc: &ProgramAccumulator<'_>,
) -> String {
    let mut s = String::new();
    s.push_str("    public static fun generated_install_middleware(): void {\n");
    s.push_str("        if !WebApp.begin_generated_middleware() {\n");
    s.push_str("            return;\n");
    s.push_str("        }\n");
    for (_, name) in middleware {
        s.push_str(&format!("        WebApp.use(Middleware({name}));\n"));
    }
    s.push_str("    }\n\n");
    s.push_str("    public static fun generated_openapi_paths(): string {\n");
    s.push_str("        return ");
    s.push_str(&dream_string(&openapi_paths(routes, json_names, acc)));
    s.push_str(";\n    }\n\n");
    s.push_str(
        "    public static async fun generated_dispatch(borrow ctx: RequestContext): HttpOutgoing {\n",
    );
    if routes.is_empty() {
        s.push_str("        return HttpOutgoing.not_found();\n");
    } else {
        s.push_str("        let method = ctx.incoming.method;\n");
        s.push_str("        let path = ctx.incoming.path;\n");
        for (i, r) in routes.iter().enumerate() {
            s.push_str(&format!(
                "        let __m{i} = WebApp.match_path({}, path);\n",
                dream_string(&r.path)
            ));
            s.push_str(&format!(
                "        if method == \"{}\" && __m{i}.is_some() {{\n",
                r.method
            ));
            s.push_str(&format!("            let __params{i} = __m{i}.unwrap();\n"));
            emit_handler_body(&mut s, r, i, acc);
            s.push_str("        }\n");
        }
        s.push_str("        return HttpOutgoing.not_found();\n");
    }
    s.push_str("    }\n");
    s
}

pub(super) fn emit_handler_body(s: &mut String, r: &Route, i: usize, acc: &ProgramAccumulator<'_>) {
    let wrap = !r.uses.is_empty();
    let ind = if wrap {
        "                "
    } else {
        "            "
    };
    if wrap {
        s.push_str("            let __leaf: fun(): Future<HttpOutgoing> = async () => {\n");
    }
    emit_extractors(s, r, i, acc, ind);
    if wrap {
        s.push_str("            };\n");
        s.push_str("            let __uses = List<Middleware>();\n");
        for u in &r.uses {
            s.push_str(&format!("            __uses.push(Middleware({u}));\n"));
        }
        s.push_str(
            "            return WebApp.run_local_middleware(ctx, __uses, 0, __leaf).await;\n",
        );
    }
}

pub(super) fn emit_return(s: &mut String, ind: &str, ret: &str, call: &str) {
    if ret == "HttpOutgoing" {
        s.push_str(&format!("{ind}return {call};\n"));
        return;
    }
    if ret == "void" {
        s.push_str(&format!("{ind}{call};\n"));
        s.push_str(&format!("{ind}return HttpOutgoing.empty(204);\n"));
        return;
    }
    if ret == "HttpStatus" {
        s.push_str(&format!("{ind}return HttpOutgoing.from_status({call});\n"));
        return;
    }
    if ret == "EventStream" {
        s.push_str(&format!("{ind}let __es = {call};\n"));
        s.push_str(&format!("{ind}__es.end();\n"));
        s.push_str(&format!("{ind}return HttpOutgoing.already_sent();\n"));
        return;
    }
    s.push_str(&format!("{ind}let __out = {call};\n"));
    if let Some((ok, err)) = result_parts(ret) {
        if err == "HttpStatus" {
            s.push_str(&format!(
                "{ind}if __out.is_err() {{ return HttpOutgoing.from_status(__out.unwrap_err()); }}\n"
            ));
            s.push_str(&format!("{ind}let __ok = __out.unwrap();\n"));
            emit_value_return(s, ind, ok, "__ok");
            return;
        }
        if err == "string" {
            s.push_str(&format!(
                "{ind}if __out.is_err() {{ return HttpOutgoing.detail(__out.unwrap_err(), 500); }}\n"
            ));
        } else {
            s.push_str(&format!(
                "{ind}if __out.is_err() {{ return HttpOutgoing.detail(__out.unwrap_err().to_string(), 500); }}\n"
            ));
        }
        s.push_str(&format!("{ind}let __ok = __out.unwrap();\n"));
        emit_value_return(s, ind, ok, "__ok");
        return;
    }
    emit_value_return(s, ind, ret, "__out");
}

pub(super) fn emit_value_return(s: &mut String, ind: &str, ty: &str, expr: &str) {
    if ty == "string" {
        s.push_str(&format!("{ind}return HttpOutgoing.text({expr}, 200);\n"));
    } else if ty == "HttpOutgoing" {
        s.push_str(&format!("{ind}return {expr};\n"));
    } else {
        s.push_str(&format!(
            "{ind}return HttpOutgoing.json_text(Json.serialize({expr}), 200);\n"
        ));
    }
}
