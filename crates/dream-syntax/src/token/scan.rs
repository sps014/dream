use super::syntax_token::SyntaxToken;
use super::token_kind::TokenKind;

/// Scans forward from the opening `<` at `open_idx` over a balanced generic argument list,
/// tracking nesting so multi-argument and nested generics (`Pair<Box<int>, int>`,
/// `Box<Box<int>>`) are handled and `>>` counts as two closing `>`.
///
/// Returns the index of the matching closing token, or `None` if a `;` or end-of-file is hit
/// first (not a generic list). Shared by the parser (to disambiguate generic calls/instantiations
/// from `<`/`>` comparisons) and the formatter (to space type-argument regions correctly).
pub fn scan_generic_close(tokens: &[SyntaxToken], open_idx: usize) -> Option<usize> {
    let mut depth: i32 = 1;
    let mut i = open_idx + 1;
    while i < tokens.len() {
        match tokens[i].kind {
            TokenKind::SmallerThanToken => depth += 1,
            TokenKind::GreaterThanToken => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            TokenKind::ShiftRightToken => {
                depth -= 2;
                if depth <= 0 {
                    return Some(i);
                }
            }
            TokenKind::SemicolonToken | TokenKind::EndOfFileToken => return None,
            _ => {}
        }
        i += 1;
    }
    None
}
