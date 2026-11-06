use super::*;
use crate::lexer::Lexer;
use crate::nodes::{ExpressionNode, StatementNode};
use crate::token::token_kind::TokenKind;

fn initializer<'a>(
    code: &str,
    arena: &'a bumpalo::Bump,
) -> (Option<ExpressionNode<'a>>, DiagnosticBag) {
    let mut diagnostics = DiagnosticBag::new(None);
    let lexer = Lexer::new(format!("fun main() {{\n    let s = {code};\n}}\n"));
    let mut parser = Parser::new(lexer, arena, &mut diagnostics);
    let init = parser.parse().ok().and_then(|tree| {
        tree.get_root().functions.first().and_then(|f| match f.body.first() {
            Some(StatementNode::Declaration(_, _, init, _, _)) => Some(init.clone()),
            _ => None,
        })
    });
    (init, diagnostics)
}

/// The string literal text (escaped, with quotes) a raw literal lowers to.
fn literal(code: &str) -> String {
    let arena = bumpalo::Bump::new();
    let (init, diagnostics) = initializer(code, &arena);
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
    match init {
        Some(ExpressionNode::Literal(Type::String(t))) => t.text,
        other => panic!("expected a string literal, got {other:?}"),
    }
}

fn errors(code: &str) -> Vec<String> {
    let arena = bumpalo::Bump::new();
    let (_, diagnostics) = initializer(code, &arena);
    diagnostics
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

fn lex_kinds(code: &str) -> Vec<TokenKind> {
    let mut diagnostics = DiagnosticBag::new(None);
    Lexer::new(code.to_string())
        .lex_all(&mut diagnostics)
        .iter()
        .map(|t| t.kind)
        .collect()
}

#[test]
fn raw_string_tokens_win_over_plain_strings() {
    assert_eq!(
        lex_kinds(r#""""a "b" c""""#),
        vec![TokenKind::RawStringToken, TokenKind::EndOfFileToken]
    );
    assert_eq!(
        lex_kinds(r#"$"""{x}""""#),
        vec![
            TokenKind::InterpolatedRawStringToken,
            TokenKind::EndOfFileToken
        ]
    );
    assert_eq!(
        lex_kinds(r#""" + """#),
        vec![
            TokenKind::StringToken,
            TokenKind::PlusToken,
            TokenKind::StringToken,
            TokenKind::EndOfFileToken
        ]
    );
}

#[test]
fn single_line_raw_string_keeps_quotes_and_backslashes() {
    assert_eq!(literal(r#""""say "hi" \n""""#), r#""say \"hi\" \\n""#);
}

#[test]
fn multi_line_raw_string_strips_closing_indentation() {
    let code = "\"\"\"\n        SELECT *\n          FROM users\n\n        WHERE name = \"Ada\"\n        \"\"\"";
    assert_eq!(
        literal(code),
        "\"SELECT *\n  FROM users\n\nWHERE name = \\\"Ada\\\"\""
    );
}

#[test]
fn raw_string_normalizes_crlf() {
    let code = "\"\"\"\r\n    a\r\n    b\r\n    \"\"\"";
    assert_eq!(literal(code), "\"a\nb\"");
}

#[test]
fn longer_fence_allows_triple_quotes_in_body() {
    assert_eq!(literal(r#"""""a """ b"""""#), r#""a \"\"\" b""#);
}

#[test]
fn raw_string_errors_are_reported() {
    assert!(
        errors("\"\"\"\n  a\n b\n  \"\"\"")
            .iter()
            .any(|m| m.contains("same whitespace as its closing quotes"))
    );
    assert!(
        errors("\"\"\"a\n  \"\"\"")
            .iter()
            .any(|m| m.contains("start on the line after"))
    );
    assert!(
        errors("\"\"\"\n  a\n  b\"\"\"")
            .iter()
            .any(|m| m.contains("on their own line"))
    );
    assert!(
        errors("\"\"\"never closed")
            .iter()
            .any(|m| m.contains("unterminated raw string"))
    );
}

#[test]
fn interpolated_raw_string_lowers_to_concatenation() {
    let arena = bumpalo::Bump::new();
    let code = "$\"\"\"\n        {{\"name\": \"{name}\", \"n\": {n + 1}}}\n        \"\"\"";
    let (init, diagnostics) = initializer(code, &arena);
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.diagnostics);
    let mut parts = Vec::new();
    let mut cur = init.expect("initializer");
    loop {
        match cur {
            ExpressionNode::Binary(left, _, right) => {
                parts.push(right.clone());
                cur = left.clone();
            }
            other => {
                parts.push(other);
                break;
            }
        }
    }
    parts.reverse();
    let texts: Vec<String> = parts
        .iter()
        .map(|p| match p {
            ExpressionNode::Literal(Type::String(t)) => t.text.clone(),
            ExpressionNode::Identifier(t) => format!("id:{}", t.text),
            ExpressionNode::Binary(..) => "binary".to_string(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        texts,
        vec![
            "\"\"",
            "\"{\\\"name\\\": \\\"\"",
            "id:name",
            "\"\\\", \\\"n\\\": \"",
            "binary",
            "\"}\"",
        ]
    );
}

#[test]
fn interpolated_raw_hole_spans_point_at_source() {
    let arena = bumpalo::Bump::new();
    let code = "$\"\"\"\n        value {name}\n        \"\"\"";
    let (init, _) = initializer(code, &arena);
    let Some(ExpressionNode::Binary(_, _, right)) = init else {
        panic!("expected concatenation");
    };
    let ExpressionNode::Identifier(name) = right else {
        panic!("expected the hole identifier");
    };
    let source = format!("fun main() {{\n    let s = {code};\n}}\n");
    assert_eq!(
        &source[name.position.start..name.position.end],
        "name"
    );
}

#[test]
fn fuzz_raw_string_fragments_never_panic() {
    let pieces = [
        "\"\"\"", "\"\"\"\"", "$\"\"\"", "\"", "{", "}", "{{", "\n", "\r\n", "  ", "a", "x",
    ];
    let mut seed: u64 = 0x5EED_1234_ABCD_0001;
    for _ in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let len = (seed as usize) % 24;
        let mut s = String::new();
        let mut state = seed;
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            s.push_str(pieces[(state as usize) % pieces.len()]);
        }
        let arena = bumpalo::Bump::new();
        let _ = initializer(&s, &arena);
    }
}
