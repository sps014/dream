use super::*;
use dream_syntax::nodes::AttributeArg;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use dream_text::line_text::LineText;
use dream_text::text_span::TextSpan;

fn ident(text: &str) -> SyntaxToken {
    let span = TextSpan::new((0, 0), &LineText::new(String::new()));
    SyntaxToken::new(TokenKind::IdentifierToken, span, text.to_string())
}

fn str_arg(text: &str) -> AttributeArg {
    let span = TextSpan::new((0, 0), &LineText::new(String::new()));
    AttributeArg::String(SyntaxToken::new(
        TokenKind::StringToken,
        span,
        text.to_string(),
    ))
}

fn attr(name: &str, args: &[&str]) -> AttributeNode {
    AttributeNode {
        resolved_id: compiler_attribute_module(name)
            .map(|module| compiler_attribute_identity(module, name))
            .map(std::sync::OnceLock::from)
            .unwrap_or_default(),
        name: ident(name),
        args: args
            .iter()
            .map(|a| {
                if a.starts_with('"') {
                    str_arg(a)
                } else {
                    AttributeArg::Enum(a.split('.').map(ident).collect())
                }
            })
            .collect(),
    }
}

#[test]
fn unknown_attribute_is_reported() {
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(
        &[attr("bogus", &[])],
        AttributeTarget::Method,
        &mut diagnostics,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn misapplied_attribute_is_reported() {
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(
        &[attr("json", &[])],
        AttributeTarget::Function,
        &mut diagnostics,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn wrong_arg_count_is_reported() {
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(
        &[attr("intrinsic", &[])],
        AttributeTarget::ExternFunction,
        &mut diagnostics,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn duplicate_non_repeatable_attribute_is_reported() {
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(
        &[attr("inline", &[]), attr("inline", &[])],
        AttributeTarget::Method,
        &mut diagnostics,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn inline_is_accepted_on_methods_and_rejected_on_externs() {
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(
        &[attr("inline", &[])],
        AttributeTarget::Method,
        &mut diagnostics,
    );
    validate_attributes(
        &[attr("inline", &[])],
        AttributeTarget::StaticMethod,
        &mut diagnostics,
    );
    validate_attributes(
        &[attr("inline", &[])],
        AttributeTarget::Function,
        &mut diagnostics,
    );
    assert!(!diagnostics.has_errors());
    validate_attributes(
        &[attr("inline", &[])],
        AttributeTarget::ExternFunction,
        &mut diagnostics,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn well_formed_attribute_is_accepted() {
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(
        &[attr("intrinsic", &["Intrinsic.Print"])],
        AttributeTarget::ExternFunction,
        &mut diagnostics,
    );
    assert!(!diagnostics.has_errors());
}

#[test]
fn c_import_target_extracts_lib_and_symbol() {
    let attrs = &[attr("c", &["\"sqlite3\"", "\"sqlite3_open\""])];
    assert_eq!(
        c_import_target(attrs),
        Some(("sqlite3".to_string(), "sqlite3_open".to_string()))
    );
    assert!(has_c_attr(attrs));
    assert_eq!(
        extern_import_target(attrs, "fallback"),
        ("c/sqlite3".to_string(), "sqlite3_open".to_string())
    );
}

#[test]
fn extern_binding_conflict_detected() {
    let attrs = &[
        attr("c", &["\"sqlite3\"", "\"sqlite3_open\""]),
        attr("js", &["\"Dream\"", "\"open\""]),
    ];
    assert!(extern_binding_conflict(attrs));
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(attrs, AttributeTarget::ExternFunction, &mut diagnostics);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

#[test]
fn runtime_import_targets_dream_module() {
    let attrs = &[attr("runtime", &["\"fileRead\""])];
    assert_eq!(runtime_import_field(attrs).as_deref(), Some("fileRead"));
    assert_eq!(
        extern_import_target(attrs, "fallback"),
        ("Dream".to_string(), "fileRead".to_string())
    );
    assert!(!extern_binding_conflict(attrs));
}

#[test]
fn runtime_conflicts_with_js() {
    let attrs = &[
        attr("runtime", &["\"fileRead\""]),
        attr("js", &["\"Dream\"", "\"fileRead\""]),
    ];
    assert!(extern_binding_conflict(attrs));
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(attrs, AttributeTarget::ExternFunction, &mut diagnostics);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

#[test]
fn c_attr_is_available_on_native_and_wasm_runtimes() {
    let attrs = &[attr("c", &["\"m\"", "\"f\""])];
    let support = RuntimeSupport::from_attributes(attrs);
    assert!(support.native);
    assert!(support.node);
    assert!(support.web);
}

#[test]
fn c_with_native_is_accepted() {
    let attrs = &[attr("c", &["\"m\"", "\"f\""]), attr("native", &[])];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(!diagnostics.has_errors());
    let support = RuntimeSupport::from_attributes(attrs);
    assert!(support.native);
    assert!(!support.node);
    assert!(!support.web);
}

#[test]
fn c_with_web_or_node_restricts_runtime_availability() {
    for host in ["web", "node"] {
        let attrs = &[attr("c", &["\"m\"", "\"f\""]), attr(host, &[])];
        let mut diagnostics = DiagnosticBag::new(None);
        validate_c_extern_attrs(attrs, &mut diagnostics);
        assert!(!diagnostics.has_errors());
        let support = RuntimeSupport::from_attributes(attrs);
        assert!(!support.native);
        assert_eq!(support.web, host == "web");
        assert_eq!(support.node, host == "node");
    }
}

#[test]
fn marshal_without_c_is_rejected() {
    // `@marshal` only affects `@c` externs — attaching it to a plain (or `@js`) extern is a
    // no-op and probably a bug, so the validator flags it.
    let attrs = &[attr("marshal", &["MarshalKind.LpWStr"])];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(attrs, AttributeTarget::ExternFunction, &mut diagnostics);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

#[test]
fn c_call_without_c_is_rejected() {
    let attrs = &[attr("c_call", &["CCallConv.Stdcall"])];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(attrs, AttributeTarget::ExternFunction, &mut diagnostics);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

#[test]
fn c_with_marshal_and_cdecl_is_accepted() {
    let attrs = &[
        attr("c", &["\"user32\"", "\"MessageBoxW\""]),
        attr("marshal", &["MarshalKind.LpWStr"]),
        attr("c_call", &["CCallConv.Cdecl"]),
    ];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(!diagnostics.has_errors());
}

#[test]
fn c_call_stdcall_is_accepted() {
    let attrs = &[
        attr("c", &["\"user32\"", "\"MessageBoxW\""]),
        attr("c_call", &["CCallConv.Stdcall"]),
    ];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(!diagnostics.has_errors());
    assert_eq!(c_call_conv(attrs), CCallConv::Stdcall);
}

#[test]
fn c_call_unknown_convention_is_rejected() {
    let attrs = &[
        attr("c", &["\"user32\"", "\"MessageBoxW\""]),
        attr("c_call", &["CCallConv.Fastcall"]),
    ];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_attributes(attrs, AttributeTarget::ExternFunction, &mut diagnostics);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

fn catalog() -> &'static UserAttributes {
    static CATALOG: std::sync::OnceLock<UserAttributes> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let arena = bumpalo::Bump::new();
        let mut diagnostics = DiagnosticBag::new(None);
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut modules = std::collections::BTreeMap::new();
        for (module, source) in [
            (
                "system.core",
                include_str!("../../../dream-stdlib/src/system/core/attributes.dream"),
            ),
            (
                "system.core",
                include_str!("../../../dream-stdlib/src/system/core/intrinsic.dream"),
            ),
            (
                "system.interop",
                include_str!("../../../dream-stdlib/src/system/interop/attributes.dream"),
            ),
            (
                "system.testing",
                include_str!("../../../dream-stdlib/src/system/testing/attributes.dream"),
            ),
        ] {
            let file = format!("{module}-{}", structs.len());
            let mut parser = dream_syntax::parser::Parser::new(
                dream_syntax::lexer::Lexer::new(source.to_string()),
                &arena,
                &mut diagnostics,
            );
            let ast = parser.parse().unwrap();
            modules.insert(file.clone(), module.to_string());
            for s in &ast.get_root().structs {
                let mut s = s.clone();
                s.file_path = Some(file.as_str().into());
                structs.push(s);
            }
            for e in &ast.get_root().enums {
                let mut e = e.clone();
                e.file_path = Some(file.as_str().into());
                enums.push(e);
            }
        }
        let registry = UserAttributes::collect(
            &structs,
            &enums,
            &|f| f.and_then(|f| modules.get(f)).cloned().unwrap_or_default(),
            Default::default(),
            &mut diagnostics,
        );
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
        registry
    })
}

fn validate_attributes(
    attrs: &[AttributeNode],
    target: AttributeTarget,
    diagnostics: &mut DiagnosticBag,
) {
    super::validate_attributes(attrs, target, catalog(), diagnostics);
}

#[test]
fn closed_choices_reject_strings_and_wrong_enum_owners() {
    for value in ["\"cdecl\"", "Other.Cdecl", "CCallConv.Unknown"] {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("c_call", &[value])],
            AttributeTarget::ExternFunction,
            &mut diagnostics,
        );
        assert!(diagnostics.has_errors(), "{value}");
    }
}

#[test]
fn intrinsic_catalog_matches_compiler_handlers() {
    let members = catalog().enum_members("system.core::Intrinsic");
    let keys: Vec<_> = members
        .iter()
        .map(|m| {
            crate::intrinsics::intrinsic_member_key(m).expect("intrinsic member has a handler")
        })
        .collect();
    assert_eq!(
        keys,
        crate::intrinsics::INTRINSIC_MEMBERS
            .iter()
            .map(|(_, key)| *key)
            .collect::<Vec<_>>()
    );
}

#[test]
fn unvalidated_spelling_cannot_activate_a_compiler_handler() {
    let a = AttributeNode {
        name: ident("intrinsic"),
        args: vec![AttributeArg::Enum(vec![ident("Intrinsic"), ident("Print")])],
        resolved_id: Default::default(),
    };
    assert!(crate::intrinsics::intrinsic_key(&[a]).is_none());
}

#[test]
fn optional_binding_arguments_preserve_arities() {
    for args in [&[][..], &["\"lib\""][..], &["\"lib\"", "\"symbol\""][..]] {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_attributes(
            &[attr("c", args)],
            AttributeTarget::ExternFunction,
            &mut diagnostics,
        );
        assert!(!diagnostics.has_errors());
    }
}

fn declared_registry(source: &str) -> (UserAttributes, DiagnosticBag) {
    let arena = bumpalo::Bump::new();
    let mut diagnostics = DiagnosticBag::new(None);
    let mut parser = dream_syntax::parser::Parser::new(
        dream_syntax::lexer::Lexer::new(source.to_owned()),
        &arena,
        &mut diagnostics,
    );
    let ast = parser.parse().unwrap();
    let mut registry = UserAttributes::collect(
        &ast.get_root().structs,
        &ast.get_root().enums,
        &|_| "sample".into(),
        Default::default(),
        &mut diagnostics,
    );
    registry.enums.extend(catalog().enums.clone());
    (registry, diagnostics)
}

#[test]
fn optional_attribute_fields_and_variadics_are_validated() {
    for source in [
        "@attribute struct good { first: string; second: Option<string>; }",
        "@attribute struct many { values: List<string>; }",
    ] {
        let (_, diagnostics) = declared_registry(source);
        assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
    }
    for source in [
        "@attribute struct bad { first: Option<string>; second: string; }",
        "@attribute struct bad { first: Option<string>; rest: List<string>; }",
        "@attribute struct bad { rest: List<string>; last: string; }",
    ] {
        let (_, diagnostics) = declared_registry(source);
        assert!(diagnostics.has_errors(), "{source}");
    }
}

#[test]
fn scoped_attributes_do_not_activate_unrelated_compiler_effects() {
    let (registry, mut diagnostics) =
        declared_registry("@attribute public struct c { library: string; }");
    let node = AttributeNode {
        name: ident("c"),
        args: vec![str_arg("\"lib\"")],
        resolved_id: Default::default(),
    };
    super::validate_attributes(
        std::slice::from_ref(&node),
        AttributeTarget::ExternFunction,
        &registry,
        &mut diagnostics,
    );
    assert!(!diagnostics.has_errors());
    assert_eq!(
        node.resolved_id.get().map(String::as_str),
        Some("sample::c")
    );
    assert!(!has_c_attr(&[node]));
}

#[test]
fn specialization_requires_its_import() {
    let registry = catalog().clone().with_visibility(
        [("main".to_string(), "sample".to_string())].into(),
        [("main".to_string(), vec![])].into(),
    );
    let mut diagnostics = DiagnosticBag::new(Some("main".into()));
    super::validate_attributes(
        &[attr("c", &[])],
        AttributeTarget::ExternFunction,
        &registry,
        &mut diagnostics,
    );
    assert!(diagnostics.has_errors());
}

#[test]
fn marshaling_choices_are_context_specific() {
    for (member, parameter, accepted) in [
        ("MarshalKind.LpStr", false, true),
        ("MarshalKind.LpWStr", false, true),
        ("MarshalKind.UserDataLast", true, true),
        ("MarshalKind.LpWStr", true, false),
        ("MarshalKind.UserDataLast", false, false),
    ] {
        let mut diagnostics = DiagnosticBag::new(None);
        validate_marshal_context(&[attr("marshal", &[member])], parameter, &mut diagnostics);
        assert_eq!(!diagnostics.has_errors(), accepted, "{member}");
    }
}

#[test]
fn attribute_suffixes_and_repeatability_come_from_declarations() {
    let (registry, mut diagnostics) =
        declared_registry("@attribute @repeatable public struct RouteAttribute { path: string; }");
    let make = || AttributeNode {
        name: ident("route"),
        args: vec![str_arg("\"/users\"")],
        resolved_id: Default::default(),
    };
    let nodes = [make(), make()];
    super::validate_attributes(
        &nodes,
        AttributeTarget::Function,
        &registry,
        &mut diagnostics,
    );
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
    assert_eq!(
        nodes[0].resolved_id.get().map(String::as_str),
        Some("sample::RouteAttribute")
    );
}
