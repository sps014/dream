use dream_syntax::token::scan::scan_generic_close;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

/// Detects `<...>` regions that are generic type-argument lists rather than `<`/`>`
/// comparisons, using the same balanced scan the parser uses for disambiguation
/// ([`scan_generic_close`]).
///
/// Token-level formatting lacks the parser's grammar context, so detection combines three
/// signals; when any is missing the region is left as a comparison (spacing-only difference,
/// never a correctness issue):
///
/// 1. the token before `<` can end a type position (identifier, builtin type, `)`/`]`, or a
///    closing `>` of an enclosing generic) — never a literal/operator;
/// 2. every token strictly inside is type grammar (identifiers, builtins, `,`, `[`/`]`,
///    nested `<`/`>`/`>>`) — `a < b && c > d` fails on `&&`;
/// 3. what follows the matching close looks like it continues from a *type*, not an operand.
pub(super) fn detect_type_arg_regions(tokens: &[SyntaxToken]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    for i in 1..tokens.len() {
        if tokens[i].kind != TokenKind::SmallerThanToken {
            continue;
        }
        if !matches!(
            tokens[i - 1].kind,
            TokenKind::IdentifierToken
                | TokenKind::DataTypeToken
                | TokenKind::GreaterThanToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
        ) {
            continue;
        }
        let Some(close) = scan_generic_close(tokens, i) else {
            continue;
        };
        if !region_is_type_like(tokens, i, close) {
            continue;
        }
        if !close_continues_a_type(tokens, i, close) {
            continue;
        }
        regions.push((i, close));
    }
    regions
}

fn region_is_type_like(tokens: &[SyntaxToken], open: usize, close: usize) -> bool {
    // `:`/`+` occur inside generic *parameter* declarations (`<T : Comparable<T> + Shared>`),
    // which the formatter disambiguates through this same path.
    tokens[open + 1..close].iter().all(|t| {
        matches!(
            t.kind,
            TokenKind::IdentifierToken
                | TokenKind::DataTypeToken
                | TokenKind::CommaToken
                | TokenKind::ColonToken
                | TokenKind::PlusToken
                | TokenKind::OpenBracketToken
                | TokenKind::CloseBracketToken
                | TokenKind::SmallerThanToken
                | TokenKind::GreaterThanToken
                | TokenKind::ShiftRightToken
        )
    })
}

/// True when at least one nested `<` appears strictly inside `(open, close)`.
fn has_nested_generic(tokens: &[SyntaxToken], open: usize, close: usize) -> bool {
    tokens[open + 1..close]
        .iter()
        .any(|t| t.kind == TokenKind::SmallerThanToken)
}

fn close_continues_a_type(tokens: &[SyntaxToken], open: usize, close: usize) -> bool {
    let Some(next) = tokens.get(close + 1) else {
        return false;
    };
    let nested = has_nested_generic(tokens, open, close);
    if paren_depth_before(tokens, open) > 0 {
        // Inside call/paren groups require stronger evidence — `f(a < b, c > d)` must stay
        // comparisons. A list closed by `)`/`,`/`]`/`(` (chained generic ctor) or real
        // nesting qualifies.
        matches!(
            next.kind,
            TokenKind::CommaToken
                | TokenKind::OpenParenthesisToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
        ) || nested
    } else {
        matches!(
            next.kind,
            TokenKind::OpenParenthesisToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CommaToken
                | TokenKind::OpenBracketToken
                | TokenKind::EqualToken
                | TokenKind::FatArrowToken
                | TokenKind::DotToken
                | TokenKind::QuestionMarkToken
                | TokenKind::ColonToken
                | TokenKind::CurlyOpenBracketToken
                | TokenKind::CurlyCloseBracketToken
                | TokenKind::IdentifierToken
        )
            || (next.kind == TokenKind::SemicolonToken && nested)
            // `field: Option<T>;` ends a type annotation directly with `;` — only possible
            // in a type position, which the backward probe confirms.
            || (next.kind == TokenKind::SemicolonToken && in_type_position(tokens, open))
    }
}

/// Walks backwards over the qualified-name chain ending just before `open_idx` and reports
/// whether it sits where a *type* is expected (after `:` annotation, nested `<...>`, a comma
/// in a parameter/type list, or a declaration keyword). Expression positions hit an operator,
/// `(`, or statement boundary and return false.
fn in_type_position(tokens: &[SyntaxToken], open_idx: usize) -> bool {
    let mut j = open_idx;
    while j > 0 {
        j -= 1;
        match tokens[j].kind {
            TokenKind::IdentifierToken
            | TokenKind::DataTypeToken
            | TokenKind::DotToken
            | TokenKind::OpenBracketToken
            | TokenKind::CloseBracketToken => continue,
            TokenKind::ColonToken
            | TokenKind::SmallerThanToken
            | TokenKind::GreaterThanToken
            | TokenKind::ShiftRightToken
            | TokenKind::CommaToken => return true,
            _ => return false,
        }
    }
    false
}

fn paren_depth_before(tokens: &[SyntaxToken], idx: usize) -> i32 {
    let mut depth = 0;
    for t in &tokens[..idx] {
        match t.kind {
            TokenKind::OpenParenthesisToken => depth += 1,
            TokenKind::CloseParenthesisToken => depth -= 1,
            _ => {}
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    fn regions(src: &str) -> Vec<(usize, usize)> {
        let tokens = lex(src).expect("fixture must lex");
        detect_type_arg_regions(&tokens)
    }

    #[test]
    fn detects_annotation_and_call_regions() {
        assert_eq!(regions("let m: Map<string, int> = a;").len(), 1);
        assert_eq!(regions("Map.new<string, int>();").len(), 1);
        assert_eq!(regions("Pair<Box<int>, string> p;").len(), 2);
    }

    #[test]
    fn comparisons_are_not_type_args() {
        assert!(regions("let b: bool = a < c && c > d;").is_empty());
        assert!(regions("if (a < b) { return; }").is_empty());
        assert!(regions("f(a < b, c > d);").is_empty());
    }

    #[test]
    fn generic_params_and_annotations() {
        // `class List<T> : Iface` — parameter list with a colon follower.
        assert_eq!(regions("public class List<T>: IndexedCollection<T> { }").len(), 2);
        // Bounds inside generic parameters; the nested bound region counts separately.
        assert_eq!(
            regions("fun f<T: Comparable<T>>(a: T): int { return 0; }").len(),
            2
        );
        // Type annotation ending at `;` (type-position probe).
        assert_eq!(regions("class A { jar: Option<T>; }").len(), 1);
    }
}
