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
        name: ident(name),
        args: args.iter().map(|a| str_arg(a)).collect(),
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
        &[attr("intrinsic", &["\"print\""])],
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
    let attrs = &[attr("marshal", &["\"lpwstr\""])];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

#[test]
fn c_call_without_c_is_rejected() {
    let attrs = &[attr("c_call", &["\"stdcall\""])];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}

#[test]
fn c_with_marshal_and_cdecl_is_accepted() {
    let attrs = &[
        attr("c", &["\"user32\"", "\"MessageBoxW\""]),
        attr("marshal", &["\"lpwstr\""]),
        attr("c_call", &["\"cdecl\""]),
    ];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(!diagnostics.has_errors());
}

#[test]
fn c_call_stdcall_is_accepted() {
    let attrs = &[
        attr("c", &["\"user32\"", "\"MessageBoxW\""]),
        attr("c_call", &["\"stdcall\""]),
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
        attr("c_call", &["\"fastcall\""]),
    ];
    let mut diagnostics = DiagnosticBag::new(None);
    validate_c_extern_attrs(attrs, &mut diagnostics);
    assert!(diagnostics.has_errors());
}
