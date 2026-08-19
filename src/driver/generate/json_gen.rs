//! Builtin `@json` derive: builds a declaration snapshot, runs the Dream `JsonGenerator`
//! harness (cached native C), and `emit_file`s the resulting `extend` source.

use super::context::GeneratorContext;
use dream_diagnostics::DiagnosticBag;
#[cfg(feature = "native")]
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::struct_node::StructDeclarationNode;
use dream_syntax::nodes::EnumDeclarationNode;
#[cfg(feature = "native")]
use dream_syntax::nodes::Type;
#[cfg(feature = "native")]
use std::collections::BTreeSet;
use std::collections::HashSet;

use crate::driver::source_loader::ProgramAccumulator;

#[cfg(feature = "native")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "native")]
use std::sync::Mutex;

#[cfg(feature = "native")]
const HARNESS_SOURCE: &str = include_str!("json_gen_harness.dream");
#[cfg(feature = "native")]
const SNAPSHOT_ENV: &str = "DREAM_JSON_GEN_SNAPSHOT";
#[cfg(feature = "native")]
const OK_MARKER: &str = "__DREAM_JSON_GEN_OK__";
#[cfg(feature = "native")]
const ERR_MARKER: &str = "__DREAM_JSON_GEN_ERR__";
#[cfg(feature = "native")]
const LOC_MARKER: &str = "__DREAM_JSON_GEN_LOC__";

/// Expands every `@json` type into synthesized `extend` source through `emit_file`.
pub fn expand_from_acc(
    ctx: &mut GeneratorContext,
    acc: &ProgramAccumulator<'_>,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
    diagnostics: &mut DiagnosticBag,
) {
    let mut json_names: HashSet<String> = structs
        .iter()
        .filter(|s| s.attributes.iter().any(|a| a.name.text == "json"))
        .map(|s| s.name.text.clone())
        .collect();
    json_names.extend(
        enums
            .iter()
            .filter(|e| e.attributes.iter().any(|a| a.name.text == "json"))
            .map(|e| e.name.text.clone()),
    );
    if json_names.is_empty() {
        #[cfg(feature = "native")]
        {
            let mut jsonable: HashSet<String> =
                structs.iter().map(|s| s.name.text.clone()).collect();
            jsonable.extend(
                enums
                    .iter()
                    .filter(|e| e.is_data_enum())
                    .map(|e| e.name.text.clone()),
            );
            let collections = collect_all_collections(acc, &jsonable);
            if collections.is_empty() {
                return;
            }
        }
        #[cfg(not(feature = "native"))]
        {
            let _ = acc;
            return;
        }
    }

    #[cfg(not(feature = "native"))]
    {
        let _ = (ctx, structs, enums);
        diagnostics.report_error(
            "@json derive requires the native compiler feature".to_string(),
            None,
        );
    }

    #[cfg(feature = "native")]
    {
        let mut jsonable: HashSet<String> = structs.iter().map(|s| s.name.text.clone()).collect();
        jsonable.extend(
            enums
                .iter()
                .filter(|e| e.is_data_enum())
                .map(|e| e.name.text.clone()),
        );

        let snapshot = build_snapshot(acc, structs, enums, &json_names, &jsonable);
        match run_dream_json_generator(&snapshot) {
            Ok(source) => {
                if !source.is_empty() {
                    ctx.emit_file("<json-derive>", source);
                }
            }
            Err(err) => {
                let span = lookup_json_error_span(&err, structs, enums);
                if let Some(path) = json_error_file_path(&err, structs, enums) {
                    diagnostics.file_path = Some(path);
                }
                diagnostics.report_error(err.message, span);
            }
        }
    }
}

#[cfg(feature = "native")]
fn build_snapshot(
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

#[cfg(feature = "native")]
fn snapshot_class(s: &StructDeclarationNode<'_>) -> String {
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
        json_escape(&s.name.text),
        json_string_array(generic_params),
        fields,
    )
}

#[cfg(feature = "native")]
fn snapshot_union(e: &EnumDeclarationNode<'_>) -> String {
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
            json_escape(&variant.name.text),
            fields
        ));
    }
    variants.push(']');
    format!(
        "{{\"name\":{},\"is_union\":true,\"generic_params\":{},\"fields\":[],\"variants\":{}}}",
        json_escape(&e.name.text),
        json_string_array(generic_params),
        variants,
    )
}

#[cfg(feature = "native")]
fn snapshot_field(
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
        json_escape(name),
        json_escape(type_name),
        if json_ignore { "true" } else { "false" },
        json_escape(&property_name),
        json_escape(&option_inner),
        if is_type_param { "true" } else { "false" },
        json_escape(&map_value_inner),
        json_escape(&map_ctor),
        json_escape(&seq_elem_inner),
        json_escape(&seq_kind),
    )
}

#[cfg(feature = "native")]
fn json_string_array(mut items: Vec<String>) -> String {
    // Deterministic order for reproducible harness input (not required for correctness).
    items.sort();
    let mut out = String::from("[");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&json_escape(s));
    }
    out.push(']');
    out
}

#[cfg(feature = "native")]
fn json_escape(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(feature = "native")]
#[derive(Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
struct CollectionSpec {
    kind: String,
    elem_type: String,
    value_type: String,
    self_ty: String,
    fn_suffix: String,
}

#[cfg(feature = "native")]
fn collection_fn_suffix(mangled: &str) -> String {
    mangled.replace("[]", "__arr")
}

#[cfg(feature = "native")]
fn json_elem_supported(name: &str, jsonable: &HashSet<String>) -> bool {
    matches!(
        name,
        "int" | "long" | "string" | "bool" | "double" | "float"
    ) || jsonable.contains(name)
}

#[cfg(feature = "native")]
fn insert_collection_spec(
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

#[cfg(feature = "native")]
fn collect_collections_from_type(
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

#[cfg(feature = "native")]
fn collect_collections_from_expr(
    expr: &ExpressionNode<'_>,
    jsonable: &HashSet<String>,
    out: &mut BTreeSet<CollectionSpec>,
) {
    match expr {
        ExpressionNode::MethodCall(receiver, method, type_args, args) => {
            collect_collections_from_expr(receiver, jsonable, out);
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out);
            }
            // Only `Json.serialize` / `deserialize` / `from_value` type args need top-level
            // collection adapters — not every `List<T>()` constructor or array annotation.
            if is_json_static_receiver(receiver)
                && (method.text == "serialize"
                    || method.text == "deserialize"
                    || method.text == "from_value")
            {
                if let Some(types) = type_args {
                    for ty in types {
                        collect_collections_from_type(ty, jsonable, out);
                    }
                }
            }
        }
        ExpressionNode::FunctionCall(_, _, args) => {
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out);
            }
        }
        ExpressionNode::Call(callee, _, args) => {
            collect_collections_from_expr(callee, jsonable, out);
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out);
            }
        }
        ExpressionNode::Binary(a, _, b) => {
            collect_collections_from_expr(a, jsonable, out);
            collect_collections_from_expr(b, jsonable, out);
        }
        ExpressionNode::Unary(_, a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::IncDec { target, .. } => {
            collect_collections_from_expr(target, jsonable, out)
        }
        ExpressionNode::Parenthesized(_, a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::IndexAccess(a, b) => {
            collect_collections_from_expr(a, jsonable, out);
            collect_collections_from_expr(b, jsonable, out);
        }
        ExpressionNode::Cast(_, _, a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::MemberAccess(a, _) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::IsExpression(a, _, _) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::Ternary(a, b, c) => {
            collect_collections_from_expr(a, jsonable, out);
            collect_collections_from_expr(b, jsonable, out);
            collect_collections_from_expr(c, jsonable, out);
        }
        ExpressionNode::Await(_, a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::Switch(_, a, arms) => {
            collect_collections_from_expr(a, jsonable, out);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_collections_from_expr(guard, jsonable, out);
                }
                match &arm.body {
                    dream_syntax::nodes::expression::SwitchArmBody::Expr(e) => {
                        collect_collections_from_expr(e, jsonable, out);
                    }
                    dream_syntax::nodes::expression::SwitchArmBody::Block(stmts) => {
                        for stmt in *stmts {
                            collect_collections_from_stmts(stmt, jsonable, out);
                        }
                    }
                }
            }
        }
        ExpressionNode::Try(a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::Lambda(lambda) => match &lambda.body {
            dream_syntax::nodes::expression::LambdaBody::Expr(e) => {
                collect_collections_from_expr(e, jsonable, out);
            }
            dream_syntax::nodes::expression::LambdaBody::Block(stmts) => {
                for stmt in *stmts {
                    collect_collections_from_stmts(stmt, jsonable, out);
                }
            }
        },
        ExpressionNode::NamedArg(_, a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::RefArgument(_, a) => collect_collections_from_expr(a, jsonable, out),
        ExpressionNode::SyntaxBlock(block) => {
            for part in &block.parts {
                if let dream_syntax::nodes::expression::SyntaxBlockPart::Splice(e) = part {
                    collect_collections_from_expr(e, jsonable, out);
                }
            }
        }
        ExpressionNode::ArrayLiteral(_, elems)
        | ExpressionNode::TupleLiteral(_, elems)
        | ExpressionNode::SetLiteral(_, elems) => {
            for elem in elems {
                collect_collections_from_expr(elem, jsonable, out);
            }
        }
        ExpressionNode::MapLiteral(_, pairs) => {
            for (k, v) in pairs {
                collect_collections_from_expr(k, jsonable, out);
                collect_collections_from_expr(v, jsonable, out);
            }
        }
        ExpressionNode::Literal(_)
        | ExpressionNode::Identifier(_)
        | ExpressionNode::SizeOf(_, _)
        | ExpressionNode::NameOf(_, _) => {}
    }
}

#[cfg(feature = "native")]
fn collect_collections_from_stmts(
    stmt: &dream_syntax::nodes::StatementNode<'_>,
    jsonable: &HashSet<String>,
    out: &mut BTreeSet<CollectionSpec>,
) {
    use dream_syntax::nodes::StatementNode;
    match stmt {
        StatementNode::ExpressionStatement(expr) | StatementNode::AwaitStmt(expr) => {
            collect_collections_from_expr(expr, jsonable, out)
        }
        StatementNode::Declaration(_, _, init, _)
        | StatementNode::TupleDeclaration { init, .. } => {
            collect_collections_from_expr(init, jsonable, out);
        }
        StatementNode::Return(expr) => {
            if let Some(e) = expr {
                collect_collections_from_expr(e, jsonable, out);
            }
        }
        StatementNode::IfElse(cond, then_body, else_ifs, else_body) => {
            collect_collections_from_expr(cond, jsonable, out);
            for s in *then_body {
                collect_collections_from_stmts(s, jsonable, out);
            }
            for (c, body) in else_ifs {
                collect_collections_from_expr(c, jsonable, out);
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out);
                }
            }
            if let Some(body) = else_body {
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out);
                }
            }
        }
        StatementNode::While(cond, body) | StatementNode::DoWhile(body, cond) => {
            collect_collections_from_expr(cond, jsonable, out);
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out);
            }
        }
        StatementNode::For(init, cond, step, body) => {
            if let Some(i) = init {
                collect_collections_from_stmts(i, jsonable, out);
            }
            if let Some(c) = cond {
                collect_collections_from_expr(c, jsonable, out);
            }
            if let Some(s) = step {
                collect_collections_from_stmts(s, jsonable, out);
            }
            for st in *body {
                collect_collections_from_stmts(st, jsonable, out);
            }
        }
        StatementNode::ForEach(_, iterable, _, _, body) => {
            collect_collections_from_expr(iterable, jsonable, out);
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out);
            }
        }
        StatementNode::Labeled(_, inner) => collect_collections_from_stmts(inner, jsonable, out),
        StatementNode::Switch(subject, arms, default_body) => {
            collect_collections_from_expr(subject, jsonable, out);
            for (labels, body) in arms {
                for label in labels {
                    collect_collections_from_expr(label, jsonable, out);
                }
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out);
                }
            }
            if let Some(body) = default_body {
                for s in *body {
                    collect_collections_from_stmts(s, jsonable, out);
                }
            }
        }
        StatementNode::Lock(target, body) => {
            collect_collections_from_expr(target, jsonable, out);
            for s in *body {
                collect_collections_from_stmts(s, jsonable, out);
            }
        }
        StatementNode::Assignment(_, rhs) | StatementNode::MemberAssignment(_, _, rhs) => {
            collect_collections_from_expr(rhs, jsonable, out);
        }
        StatementNode::IndexAssignment(a, b, rhs) => {
            collect_collections_from_expr(a, jsonable, out);
            collect_collections_from_expr(b, jsonable, out);
            collect_collections_from_expr(rhs, jsonable, out);
        }
        StatementNode::FunctionInvocation(_, _, args) => {
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out);
            }
        }
        StatementNode::MethodInvocation(receiver, method, type_args, args) => {
            if is_json_static_receiver(receiver)
                && (method.text == "serialize"
                    || method.text == "deserialize"
                    || method.text == "from_value")
            {
                if let Some(types) = type_args {
                    for ty in types {
                        collect_collections_from_type(ty, jsonable, out);
                    }
                }
            }
            for arg in args {
                collect_collections_from_expr(arg, jsonable, out);
            }
        }
        StatementNode::Break(_)
        | StatementNode::Continue(_)
        | StatementNode::WorkgroupDecl(_, _, _) => {}
    }
}

#[cfg(feature = "native")]
fn is_json_static_receiver(expr: &ExpressionNode<'_>) -> bool {
    matches!(expr, ExpressionNode::Identifier(tok) if tok.text == "Json")
}

#[cfg(feature = "native")]
fn is_user_source(path: Option<&std::rc::Rc<str>>) -> bool {
    match path {
        Some(p) => !p.starts_with("<std>/"),
        None => true,
    }
}

/// Top-level collection adapters are only needed for `Json.serialize` / `deserialize` /
/// `from_value` type arguments. `@json` field collections are inlined in generated `to_json`.
#[cfg(feature = "native")]
fn collect_all_collections(
    acc: &ProgramAccumulator<'_>,
    jsonable: &HashSet<String>,
) -> Vec<CollectionSpec> {
    let mut out = BTreeSet::new();
    for g in &acc.all_globals {
        if !is_user_source(g.file_path.as_ref()) {
            continue;
        }
        collect_collections_from_expr(&g.initializer, jsonable, &mut out);
    }
    for f in &acc.all_functions {
        if !is_user_source(f.file_path.as_ref()) {
            continue;
        }
        for stmt in f.body {
            collect_collections_from_stmts(stmt, jsonable, &mut out);
        }
    }
    out.into_iter().collect()
}

#[cfg(feature = "native")]
fn snapshot_collection(c: &CollectionSpec) -> String {
    format!(
        "{{\"kind\":{},\"elem_type\":{},\"value_type\":{},\"self_ty\":{},\"fn_suffix\":{}}}",
        json_escape(&c.kind),
        json_escape(&c.elem_type),
        json_escape(&c.value_type),
        json_escape(&c.self_ty),
        json_escape(&c.fn_suffix),
    )
}

#[cfg(feature = "native")]
struct JsonGenError {
    message: String,
    type_name: Option<String>,
    field_name: Option<String>,
}

#[cfg(feature = "native")]
fn run_dream_json_generator(snapshot: &str) -> Result<String, JsonGenError> {
    // `System.env_or` reads process env, so concurrent compiles in this process must not
    // overlap `set_var`. Snapshot *files* are unique so another `dream` (LSP, bench) cannot
    // overwrite our input the way a shared `snapshot.json` did.
    static SNAPSHOT_GUARD: Mutex<()> = Mutex::new(());
    let _guard = SNAPSHOT_GUARD.lock().unwrap_or_else(|e| e.into_inner());

    let c_path = cached_harness_c().map_err(|e| JsonGenError {
        message: e,
        type_name: None,
        field_name: None,
    })?;
    let snap_path = write_unique_snapshot(&c_path, snapshot)?;

    std::env::set_var(SNAPSHOT_ENV, snap_path.as_os_str());
    let output = crate::execution::native_c::compile_and_capture(
        &c_path,
        crate::driver::wasm_opt::OptLevel::O3,
    );
    std::env::remove_var(SNAPSHOT_ENV);
    let _ = std::fs::remove_file(&snap_path);

    let output = output.map_err(|e| JsonGenError {
        message: format!("@json generator: failed to run Dream harness: {e}"),
        type_name: None,
        field_name: None,
    })?;

    parse_generator_output(&output)
}

#[cfg(feature = "native")]
fn write_unique_snapshot(
    c_path: &str,
    snapshot: &str,
) -> Result<std::path::PathBuf, JsonGenError> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = std::path::Path::new(c_path)
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!(
        "snapshot-{}-{}-{}.json",
        std::process::id(),
        nanos,
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, snapshot.as_bytes()).map_err(|e| JsonGenError {
        message: format!("@json generator: failed to write snapshot: {e}"),
        type_name: None,
        field_name: None,
    })?;
    Ok(path)
}

#[cfg(feature = "native")]
fn parse_generator_output(output: &str) -> Result<String, JsonGenError> {
    let trimmed = output.trim_start();
    if let Some(rest) = trimmed.strip_prefix(OK_MARKER) {
        let source = rest.strip_prefix('\n').unwrap_or(rest);
        return Ok(source.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix(ERR_MARKER) {
        let body = rest.trim_start_matches('\n');
        let (msg_part, loc_part) = if let Some((m, l)) = body.split_once(LOC_MARKER) {
            (m.trim(), Some(l.trim()))
        } else {
            (body.trim(), None)
        };
        let message = if msg_part.is_empty() {
            "@json generator failed".to_string()
        } else {
            // Keep the first line as the user-facing message (LOC is separate).
            msg_part.lines().next().unwrap_or(msg_part).to_string()
        };
        let (type_name, field_name) = if let Some(loc) = loc_part {
            let mut parts = loc.splitn(2, '\t');
            let ty = parts.next().unwrap_or("").trim();
            let field = parts.next().unwrap_or("").trim();
            (
                if ty.is_empty() {
                    None
                } else {
                    Some(ty.to_string())
                },
                if field.is_empty() {
                    None
                } else {
                    Some(field.to_string())
                },
            )
        } else {
            (
                extract_quoted_after(msg_part, "class '")
                    .or_else(|| extract_quoted_after(msg_part, "union '")),
                extract_quoted_after(msg_part, "field '"),
            )
        };
        return Err(JsonGenError {
            message,
            type_name,
            field_name,
        });
    }
    Err(JsonGenError {
        message: format!("@json generator: unexpected harness output: {output}"),
        type_name: None,
        field_name: None,
    })
}

#[cfg(feature = "native")]
fn extract_quoted_after(msg: &str, prefix: &str) -> Option<String> {
    let start = msg.find(prefix)? + prefix.len();
    let rest = &msg[start..];
    let end = rest.find('\'')?;
    Some(rest[..end].to_string())
}

#[cfg(feature = "native")]
fn json_error_file_path(
    err: &JsonGenError,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
) -> Option<String> {
    let type_name = err.type_name.as_deref()?;
    for s in structs {
        if s.name.text == type_name {
            return s.file_path.as_ref().map(|p| p.to_string());
        }
    }
    for e in enums {
        if e.name.text == type_name {
            return e.file_path.as_ref().map(|p| p.to_string());
        }
    }
    None
}

#[cfg(feature = "native")]
fn lookup_json_error_span(
    err: &JsonGenError,
    structs: &[StructDeclarationNode<'_>],
    enums: &[EnumDeclarationNode<'_>],
) -> Option<dream_text::text_span::TextSpan> {
    let type_name = err.type_name.as_deref()?;
    if let Some(field_name) = err.field_name.as_deref() {
        for s in structs {
            if s.name.text == type_name {
                for f in &s.fields {
                    if f.name.text == field_name {
                        return Some(f.name.position);
                    }
                }
                return Some(s.name.position);
            }
        }
        for e in enums {
            if e.name.text == type_name {
                for v in &e.variants {
                    for f in &v.fields {
                        if f.name.text == field_name {
                            return Some(f.name.position);
                        }
                    }
                }
                return Some(e.name.position);
            }
        }
    } else {
        for s in structs {
            if s.name.text == type_name {
                return Some(s.name.position);
            }
        }
        for e in enums {
            if e.name.text == type_name {
                return Some(e.name.position);
            }
        }
    }
    None
}

#[cfg(feature = "native")]
fn fnv1a(parts: &[&str]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

#[cfg(feature = "native")]
fn cached_harness_c() -> Result<String, String> {
    let fingerprint = fnv1a(&[
        HARNESS_SOURCE,
        include_str!("../../../crates/dream-stdlib/src/system/json/json_generator.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/gen_result.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/gen_field.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/gen_collection.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/gen_variant.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/gen_type.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/codegen/codegen.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/text/string_builder.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/json_value.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/json.dream"),
        include_str!("../../../crates/dream-stdlib/src/system/json/json_parser.dream"),
        include_str!("../../../crates/dream-mir/src/passes/rc/insertion.rs"),
        include_str!("../../../crates/dream-mir/src/passes/rc/uniqueness.rs"),
        include_str!("../../../crates/dream-mir/src/passes/rc/tokens.rs"),
        include_str!("../../../crates/dream-mir/src/backend/c/rvalue.rs"),
        include_str!("../../../crates/dream-mir/src/backend/c/module.rs"),
        include_str!("../../../crates/dream-mir/src/backend/c/print.rs"),
        &format!(
            "{}:{}",
            dream_mir::abi::STRING_HEADER_SIZE,
            dream_mir::abi::STRING_UNITS_OFFSET
        ),
    ]);
    let entry = super::current_entry_file();
    let dir = super::manifest::harness_cache_dir(entry.as_deref(), "json-gen-harness", fingerprint);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("@json generator: create harness dir: {e}"))?;
    let lock_path = dir.join(".lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("@json generator: lock harness dir: {e}"))?;
    lock_file
        .lock()
        .map_err(|e| format!("@json generator: lock harness dir: {e}"))?;
    let src_path = dir.join("harness.dream");
    let c_path = dir.join("harness.c");
    if !c_path.is_file() {
        std::fs::write(&src_path, HARNESS_SOURCE)
            .map_err(|e| format!("@json generator: write harness source: {e}"))?;
        let src = src_path.to_string_lossy().into_owned();
        let out = c_path.to_string_lossy().into_owned();
        let compiler =
            crate::driver::compiler::Compiler::new(crate::driver::compiler::Target::NativeC)
                .with_skip_generators(true)
                .with_release(true)
                .with_optimize(None);
        compiler
            .compile(&src, &out)
            .map_err(|_| "@json generator: failed to compile Dream harness".to_string())?;
    }
    Ok(c_path.to_string_lossy().into_owned())
}
