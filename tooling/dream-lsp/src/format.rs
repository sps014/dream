//! Dream source pretty-printer for LSP Format Document.
//!
//! Walks the lexer token stream (with attached comment trivia) and rewrites only inter-token
//! whitespace / newlines / indentation. Token text and comments are preserved verbatim — Dream's
//! AST drops punctuation and trailing comments on `;`/`}`, so a full AST round-trip cannot keep
//! comments faithfully.

use dream::diagnostics::DiagnosticBag;
use dream::syntax::lexer::Lexer;
use dream::syntax::token::syntax_token::SyntaxToken;
use dream::syntax::token::syntax_trivia::SyntaxTrivia;
use dream::syntax::token::token_kind::TokenKind;

const INDENT_UNIT: &str = "    ";

/// Pretty-prints `text`. On a failed/empty lex, returns the input with trailing whitespace trimmed
/// and a single trailing newline (never invents tokens).
pub fn format(text: &str) -> String {
    let mut diagnostics = DiagnosticBag::new(None);
    let mut lexer = Lexer::new(text.to_string());
    let tokens = lexer.lex_all(&mut diagnostics);
    if tokens.is_empty()
        || tokens
            .iter()
            .all(|t| matches!(t.kind, TokenKind::EndOfFileToken))
            && tokens
                .first()
                .map(|t| t.leading_trivia.is_empty())
                .unwrap_or(true)
    {
        return ensure_trailing_newline(text.trim_end());
    }
    let mut printer = Printer::new();
    printer.print_tokens(&tokens);
    printer.finish()
}

fn ensure_trailing_newline(s: &str) -> String {
    let mut out = s.to_string();
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

struct Printer {
    out: String,
    /// Brace nesting used for indentation.
    brace_depth: i32,
    /// `(`/`)` nesting — `;` inside for-headers must not force a newline.
    paren_depth: i32,
    bracket_depth: i32,
    at_line_start: bool,
    /// Previous emitted token kind (not trivia).
    last_kind: Option<TokenKind>,
    /// True when the previous non-trivia emit was `{` with nothing after it yet (`{}`).
    pending_empty_brace: bool,
}

impl Printer {
    fn new() -> Self {
        Self {
            out: String::new(),
            brace_depth: 0,
            paren_depth: 0,
            bracket_depth: 0,
            at_line_start: true,
            last_kind: None,
            pending_empty_brace: false,
        }
    }

    fn finish(mut self) -> String {
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push('\n');
        self.out
    }

    fn print_tokens(&mut self, tokens: &[SyntaxToken]) {
        for token in tokens {
            if token.kind == TokenKind::EndOfFileToken {
                self.emit_leading_trivia(&token.leading_trivia);
                break;
            }
            if token.kind == TokenKind::BadToken || token.text.is_empty() {
                continue;
            }
            self.emit_leading_trivia(&token.leading_trivia);
            self.emit_token(token);
            self.emit_trailing_trivia(&token.trailing_trivia);
        }
    }

    fn emit_leading_trivia(&mut self, trivia: &[SyntaxTrivia]) {
        for t in trivia {
            match t.kind {
                TokenKind::LineCommentToken => {
                    if !self.at_line_start && !self.out.is_empty() {
                        self.newline();
                    }
                    self.emit_indent();
                    self.out.push_str(t.text.trim_end());
                    self.newline();
                }
                TokenKind::BlockCommentToken => {
                    if !self.at_line_start {
                        self.space();
                    } else {
                        self.emit_indent();
                    }
                    self.emit_block_comment(&t.text);
                    self.newline();
                }
                _ => {}
            }
        }
    }

    fn emit_trailing_trivia(&mut self, trivia: &[SyntaxTrivia]) {
        for t in trivia {
            match t.kind {
                TokenKind::LineCommentToken => {
                    self.space();
                    self.out.push_str(t.text.trim_end());
                    // Trailing line comments end the line.
                    self.newline();
                }
                TokenKind::BlockCommentToken => {
                    self.space();
                    self.emit_block_comment(&t.text);
                }
                _ => {}
            }
        }
    }

    fn emit_block_comment(&mut self, text: &str) {
        // Keep block-comment text; normalize only the indent of continuation lines to current depth.
        let lines: Vec<&str> = text.split('\n').collect();
        if lines.len() == 1 {
            self.out.push_str(text.trim_end());
            self.at_line_start = false;
            return;
        }
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                self.out.push_str(line.trim_end());
                self.out.push('\n');
                self.at_line_start = true;
            } else if i + 1 == lines.len() {
                self.emit_indent();
                self.out.push_str(line.trim());
                self.at_line_start = false;
            } else {
                self.emit_indent();
                // Preserve a leading `*` alignment commonly used in block comments.
                let trimmed = line.trim_start();
                self.out.push_str(trimmed.trim_end());
                self.out.push('\n');
                self.at_line_start = true;
            }
        }
    }

    fn emit_token(&mut self, token: &SyntaxToken) {
        let kind = token.kind;

        // Blank line between top-level declarations after a closing `}` or `;`.
        if self.brace_depth == 0
            && self.paren_depth == 0
            && matches!(
                self.last_kind,
                Some(TokenKind::CurlyCloseBracketToken | TokenKind::SemicolonToken)
            )
            && is_decl_starter(kind)
            && !self.at_line_start
        {
            self.newline();
            self.newline();
        } else if self.brace_depth == 0
            && self.paren_depth == 0
            && matches!(
                self.last_kind,
                Some(TokenKind::CurlyCloseBracketToken | TokenKind::SemicolonToken)
            )
            && is_decl_starter(kind)
            && self.at_line_start
            && self.out.ends_with('\n')
            && !self.out.ends_with("\n\n")
            && !self.out.is_empty()
        {
            // Already at line start after `}` — insert one blank line.
            self.out.push('\n');
        }

        // `}`: decrease depth before indenting the line that holds it.
        if kind == TokenKind::CurlyCloseBracketToken {
            self.brace_depth = (self.brace_depth - 1).max(0);
            if self.pending_empty_brace {
                // `{}` — keep closing brace on the same line.
                self.pending_empty_brace = false;
            } else if !self.at_line_start {
                self.newline();
            }
        }

        // `else` / `while` after `}` stay on the same line: `} else {` / `} while (`.
        if matches!(kind, TokenKind::ElseToken | TokenKind::WhileToken)
            && self.last_kind == Some(TokenKind::CurlyCloseBracketToken)
            && self.at_line_start
        {
            // Undo the newline before `}`'s following keyword — pull back onto previous line.
            while self.out.ends_with('\n') {
                self.out.pop();
            }
            self.at_line_start = false;
            self.space();
        }

        if let Some(prev) = self.last_kind {
            if needs_space(prev, kind) && !self.at_line_start {
                self.space();
            }
        }

        self.emit_indent();
        self.out.push_str(&token.text);
        self.at_line_start = false;
        self.pending_empty_brace = kind == TokenKind::CurlyOpenBracketToken;

        match kind {
            TokenKind::CurlyOpenBracketToken => {
                self.brace_depth += 1;
                self.newline();
            }
            TokenKind::OpenParenthesisToken => {
                self.paren_depth += 1;
            }
            TokenKind::CloseParenthesisToken => {
                self.paren_depth = (self.paren_depth - 1).max(0);
            }
            TokenKind::OpenBracketToken => {
                self.bracket_depth += 1;
            }
            TokenKind::CloseBracketToken => {
                self.bracket_depth = (self.bracket_depth - 1).max(0);
            }
            TokenKind::SemicolonToken => {
                if self.paren_depth == 0 && self.bracket_depth == 0 {
                    self.newline();
                }
            }
            TokenKind::CurlyCloseBracketToken => {
                // After a top-level / block close, break unless `else`/`while` will join (handled
                // when those tokens arrive). Default: newline.
                self.newline();
            }
            _ => {
                self.pending_empty_brace = false;
            }
        }

        self.last_kind = Some(kind);
    }

    fn newline(&mut self) {
        if !self.out.ends_with('\n') && !self.out.is_empty() {
            self.out.push('\n');
        } else if self.out.is_empty() {
            // nothing
        }
        self.at_line_start = true;
    }

    fn space(&mut self) {
        if !self.at_line_start && !self.out.ends_with(' ') && !self.out.ends_with('\n') {
            self.out.push(' ');
        }
    }

    fn emit_indent(&mut self) {
        if self.at_line_start {
            for _ in 0..self.brace_depth {
                self.out.push_str(INDENT_UNIT);
            }
            self.at_line_start = false;
        }
    }
}

fn is_decl_starter(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::FunToken
            | TokenKind::ClassToken
            | TokenKind::StructToken
            | TokenKind::EnumToken
            | TokenKind::InterfaceToken
            | TokenKind::ExtendToken
            | TokenKind::ImportToken
            | TokenKind::ModuleToken
            | TokenKind::LetToken
            | TokenKind::ConstToken
            | TokenKind::PublicToken
            | TokenKind::InternalToken
            | TokenKind::ExternToken
            | TokenKind::StaticToken
            | TokenKind::AsyncToken
            | TokenKind::SealedToken
            | TokenKind::UnmanagedToken
            | TokenKind::RefToken
            | TokenKind::AtToken
            | TokenKind::TypeToken
    )
}

fn is_binary_op(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::PlusToken
            | TokenKind::MinusToken
            | TokenKind::StarToken
            | TokenKind::SlashToken
            | TokenKind::ModulusToken
            | TokenKind::EqualEqualToken
            | TokenKind::NotEqualToken
            | TokenKind::GreaterThanToken
            | TokenKind::GreaterThanEqualToken
            | TokenKind::SmallerThanToken
            | TokenKind::SmallerThanEqualToken
            | TokenKind::AmpersandAmpersandToken
            | TokenKind::PipePipeToken
            | TokenKind::BitWisePipeToken
            | TokenKind::BitWiseAmpersandToken
            | TokenKind::BitWiseXorToken
            | TokenKind::ShiftLeftToken
            | TokenKind::ShiftRightToken
            | TokenKind::QuestionQuestionToken
            | TokenKind::EqualToken
            | TokenKind::PlusEqualToken
            | TokenKind::MinusEqualToken
            | TokenKind::StarEqualToken
            | TokenKind::SlashEqualToken
            | TokenKind::ModulusEqualToken
            | TokenKind::FatArrowToken
            | TokenKind::IsToken
            | TokenKind::AsToken
            | TokenKind::InToken
            | TokenKind::DotDotToken
    )
}

fn is_prefix_unary(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::BangToken
            | TokenKind::TildeToken
            | TokenKind::PlusPlusToken
            | TokenKind::MinusMinusToken
            | TokenKind::AwaitToken
            | TokenKind::RefToken
            | TokenKind::BorrowToken
    )
}

fn needs_space(prev: TokenKind, next: TokenKind) -> bool {
    // Never space before closers / separators / postfix.
    if matches!(
        next,
        TokenKind::CommaToken
            | TokenKind::SemicolonToken
            | TokenKind::CloseParenthesisToken
            | TokenKind::CloseBracketToken
            | TokenKind::CurlyCloseBracketToken
            | TokenKind::DotToken
            | TokenKind::PlusPlusToken
            | TokenKind::MinusMinusToken
            | TokenKind::QuestionMarkToken
    ) {
        return false;
    }

    // Never space after openers / `@` / `.` / `...`.
    if matches!(
        prev,
        TokenKind::OpenParenthesisToken
            | TokenKind::OpenBracketToken
            | TokenKind::DotToken
            | TokenKind::DotDotDotToken
            | TokenKind::AtToken
    ) {
        return false;
    }

    // `foo(` / `foo[` / `foo<` generics — no space before `(` `[` when previous is ident/keyword/closer.
    if matches!(
        next,
        TokenKind::OpenParenthesisToken | TokenKind::OpenBracketToken
    ) && matches!(
        prev,
        TokenKind::IdentifierToken
            | TokenKind::CloseParenthesisToken
            | TokenKind::CloseBracketToken
            | TokenKind::GreaterThanToken
            | TokenKind::DataTypeToken
            | TokenKind::BooleanToken
            | TokenKind::StringToken
            | TokenKind::InterpolatedStringToken
            | TokenKind::CharToken
            | TokenKind::NumberToken
            | TokenKind::CurlyCloseBracketToken
    ) {
        return false;
    }

    // Prefix unary: `!x` `~x` `++x` `--x` `await x` — space after await/ref/borrow, not after !/~.
    if is_prefix_unary(prev) {
        return matches!(
            prev,
            TokenKind::AwaitToken | TokenKind::RefToken | TokenKind::BorrowToken
        );
    }

    // Space after `,` `:` `;` (`;` usually already newline'd).
    if matches!(
        prev,
        TokenKind::CommaToken | TokenKind::ColonToken | TokenKind::SemicolonToken
    ) {
        return true;
    }

    // Space around binary operators.
    if is_binary_op(prev) || is_binary_op(next) {
        return true;
    }

    // Keywords that introduce a following expression / type / name.
    if is_keyword_needing_space(prev) {
        return true;
    }

    // `} else` handled specially; still want space if somehow adjacent.
    if prev == TokenKind::CurlyCloseBracketToken {
        return !matches!(
            next,
            TokenKind::CommaToken
                | TokenKind::SemicolonToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
                | TokenKind::DotToken
        );
    }

    // Adjacent identifiers / literals / keywords → space (`return x`, `pub fun`, `static fun`).
    if is_word_like(prev) && is_word_like(next) {
        return true;
    }

    // `) {` / `] {` / `> {` / ident `{`
    if next == TokenKind::CurlyOpenBracketToken {
        return true;
    }

    false
}

fn is_keyword_needing_space(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::IfToken
            | TokenKind::ElseToken
            | TokenKind::ForToken
            | TokenKind::WhileToken
            | TokenKind::DoToken
            | TokenKind::LockToken
            | TokenKind::ReturnToken
            | TokenKind::BreakToken
            | TokenKind::ContinueToken
            | TokenKind::LetToken
            | TokenKind::ConstToken
            | TokenKind::FunToken
            | TokenKind::AsyncToken
            | TokenKind::AwaitToken
            | TokenKind::StaticToken
            | TokenKind::ImportToken
            | TokenKind::AsToken
            | TokenKind::ModuleToken
            | TokenKind::PublicToken
            | TokenKind::InternalToken
            | TokenKind::ExternToken
            | TokenKind::ClassToken
            | TokenKind::StructToken
            | TokenKind::UnmanagedToken
            | TokenKind::SealedToken
            | TokenKind::WeakToken
            | TokenKind::UnownedToken
            | TokenKind::RefToken
            | TokenKind::BorrowToken
            | TokenKind::InterfaceToken
            | TokenKind::ExtendToken
            | TokenKind::IsToken
            | TokenKind::InToken
            | TokenKind::EnumToken
            | TokenKind::TypeToken
            | TokenKind::SwitchToken
            | TokenKind::CaseToken
            | TokenKind::DefaultToken
            | TokenKind::DataTypeToken
    )
}

fn is_word_like(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::IdentifierToken
            | TokenKind::DataTypeToken
            | TokenKind::BooleanToken
            | TokenKind::NumberToken
    ) || is_keyword_needing_space(kind)
}

#[cfg(test)]
mod tests {
    use super::format;

    #[test]
    fn pretty_prints_minified_function() {
        let src = "fun main():void{let x:int=1;if(x>0){println(x);}}";
        let out = format(src);
        assert_eq!(
            out,
            "\
fun main(): void {
    let x: int = 1;
    if (x > 0) {
        println(x);
    }
}
"
        );
    }

    #[test]
    fn pretty_prints_else_on_same_line_as_brace() {
        let src = "fun main(): void { if (true) { return; } else { return; } }";
        let out = format(src);
        assert!(
            out.contains("} else {"),
            "expected `}} else {{`, got:\n{out}"
        );
    }

    #[test]
    fn preserves_line_comments() {
        let src = "// header\nfun main(): void { let x = 1; // trail\n}";
        let out = format(src);
        assert!(out.starts_with("// header\n"), "got:\n{out}");
        assert!(out.contains("// trail"), "got:\n{out}");
    }

    #[test]
    fn keeps_for_header_semicolons_inline() {
        let src = "fun main(): void { for (let i = 0; i < 10; i = i + 1) { println(i); } }";
        let out = format(src);
        assert!(
            out.contains("for (let i = 0; i < 10; i = i + 1)"),
            "for header should stay one line, got:\n{out}"
        );
    }

    #[test]
    fn blank_line_between_top_level_functions() {
        let src = "fun a(): void {}\nfun b(): void {}";
        let out = format(src);
        assert!(
            out.contains("}\n\nfun b"),
            "expected blank line between decls, got:\n{out}"
        );
    }

    #[test]
    fn spaces_around_operators_and_after_comma() {
        let src = "fun add(a:int,b:int):int{return a+b;}";
        let out = format(src);
        assert_eq!(
            out,
            "\
fun add(a: int, b: int): int {
    return a + b;
}
"
        );
    }
}
