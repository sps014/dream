use crate::generics::detect_type_arg_regions;
use crate::layout::Layout;
use crate::line_index::LineIndex;
use crate::spacing::{is_decl_starter, needs_space};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

/// Delimiter nesting. `Block` drives indentation; `Paren`/`Bracket`/`TypeArgs` suppress
/// statement newlines (`;` inside a `for` header or subscript stays inline).
#[derive(Clone, Copy)]
enum Ctx {
    Block { switch_body: bool },
    Paren,
    Bracket,
    TypeArgs,
}

pub(super) struct Printer {
    pub(super) layout: Layout,
    pub(super) line_index: LineIndex,
    ctxs: Vec<Ctx>,
    /// Indentation contributed by open blocks.
    brace_indent: usize,
    /// Armed between `switch` and its opening `{`; that block becomes a switch body whose
    /// `case`/`default` labels and bodies get dedicated indentation.
    pending_switch: bool,
    /// True while emitting statements after a `case ...:` header (one extra indent level).
    in_case_body: bool,
    /// Armed after a `case`/`default` label; the next colon switches into its body indent.
    case_colon_pending: bool,
    prev_kind: Option<TokenKind>,
    prev_prev_kind: Option<TokenKind>,
    /// Source line of the last emitted token/comment end, for blank-line preservation.
    pub(super) prev_end_line: usize,
    /// Byte offset anchoring the pending token's line: its first leading comment if any,
    /// else the token itself. Blank-line gaps are measured from here.
    pending_anchor_offset: usize,
}

impl Printer {
    pub fn new(text: &str) -> Self {
        Printer {
            layout: Layout::new(),
            line_index: LineIndex::new(text),
            ctxs: Vec::new(),
            brace_indent: 0,
            pending_switch: false,
            in_case_body: false,
            case_colon_pending: false,
            prev_kind: None,
            prev_prev_kind: None,
            prev_end_line: 0,
            pending_anchor_offset: 0,
        }
    }

    pub fn run(mut self, tokens: &[SyntaxToken]) -> String {
        let regions = detect_type_arg_regions(tokens);
        let mut next_region = 0usize;
        // Stack of `(open_idx, close_idx)` for currently-open type-argument regions; pushes are
        // ordered by open index and every close matches the innermost open region.
        let mut open_regions: Vec<(usize, usize)> = Vec::new();

        let mut i = 0usize;
        while i < tokens.len() {
            let token = &tokens[i];
            if token.kind == TokenKind::EndOfFileToken {
                self.emit_leading_trivia(token);
                break;
            }

            if next_region < regions.len() && regions[next_region].0 == i {
                open_regions.push(regions[next_region]);
                self.ctxs.push(Ctx::TypeArgs);
                next_region += 1;
            }

            if token.kind == TokenKind::CurlyOpenBracketToken
                && tokens.get(i + 1).map(|t| t.kind) == Some(TokenKind::CurlyCloseBracketToken)
                && tokens[i + 1].leading_trivia.is_empty()
            {
                // `{}` collapses onto one line (the closer carries no comments to drop).
                self.print_collapsed_empty_block(i, tokens);
                // The closer's own emission bookkeeping:
                while open_regions.last().map(|r| r.1) == Some(i) {
                    open_regions.pop();
                    self.ctxs.pop();
                }
                i += 2;
                continue;
            }

            self.print_token(i, tokens);

            while open_regions.last().map(|r| r.1) == Some(i) {
                open_regions.pop();
                self.ctxs.pop();
            }

            i += 1;
        }

        self.layout.finish()
    }

    fn print_token(&mut self, i: usize, tokens: &[SyntaxToken]) {
        let token = &tokens[i];
        let kind = token.kind;
        let next_kind = tokens
            .get(i + 1)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::EndOfFileToken);

        // Ordering matters: blank-line decisions precede comment emission so a preserved blank
        // separates the previous statement from this token's leading comments — never the
        // comments from their own code.
        self.set_anchor_offset(token);
        match kind {
            TokenKind::CurlyCloseBracketToken => {
                if let Some(Ctx::Block { .. }) = self.ctxs.last() {
                    self.ctxs.pop();
                    self.brace_indent = self.brace_indent.saturating_sub(1);
                    self.in_case_body = false;
                }
                // Take the closing brace onto its own (dedented) line; whether it breaks
                // *after* — or joins `else`/`while` — is decided post-emission.
                self.layout.break_line();
            }
            TokenKind::CaseToken | TokenKind::DefaultToken => {
                self.in_case_body = false;
                self.case_colon_pending = false;
                self.layout.break_line();
            }
            _ => {}
        }

        self.insert_blank_lines(kind);
        self.emit_leading_trivia(token);

        if self.layout.at_line_start() {
            self.layout.write_indent(self.current_indent(kind));
        } else {
            let mut space = needs_space(
                self.prev_prev_kind,
                self.prev_kind.unwrap_or(TokenKind::EndOfFileToken),
                kind,
            );
            let in_type_args = matches!(self.ctxs.last(), Some(Ctx::TypeArgs));
            if in_type_args {
                // Inside generic arguments `<`/`>` delimit a type list, not comparisons.
                if matches!(
                    kind,
                    TokenKind::SmallerThanToken
                        | TokenKind::GreaterThanToken
                        | TokenKind::ShiftRightToken
                ) || self.prev_kind == Some(TokenKind::SmallerThanToken)
                {
                    space = false;
                }
            }
            if space {
                self.layout.space();
            }
        }

        self.layout.text(&token.text);
        self.emit_trailing_trivia(token);

        match kind {
            TokenKind::CurlyCloseBracketToken => {
                // `} else {`, `do { ... } while (`, and expression statements ending in
                // `};` (switch expressions) stay on the closing line.
                if !matches!(
                    next_kind,
                    TokenKind::ElseToken | TokenKind::WhileToken | TokenKind::SemicolonToken
                ) {
                    self.layout.break_line();
                }
            }
            TokenKind::CurlyOpenBracketToken => {
                let switch_body = self.pending_switch;
                self.pending_switch = false;
                self.ctxs.push(Ctx::Block { switch_body });
                self.brace_indent += 1;
                self.layout.break_line();
            }
            TokenKind::OpenParenthesisToken => {
                self.ctxs.push(Ctx::Paren);
            }
            TokenKind::CloseParenthesisToken => {
                if matches!(self.ctxs.last(), Some(Ctx::Paren)) {
                    self.ctxs.pop();
                }
            }
            TokenKind::OpenBracketToken => {
                self.ctxs.push(Ctx::Bracket);
            }
            TokenKind::CloseBracketToken => {
                if matches!(self.ctxs.last(), Some(Ctx::Bracket)) {
                    self.ctxs.pop();
                }
            }
            TokenKind::SwitchToken => {
                self.pending_switch = true;
            }
            TokenKind::SemicolonToken => {
                self.pending_switch = false;
                if !matches!(
                    self.ctxs.last(),
                    Some(Ctx::Paren) | Some(Ctx::Bracket) | Some(Ctx::TypeArgs)
                ) {
                    self.layout.break_line();
                }
            }
            TokenKind::ColonToken => {
                if self.case_colon_pending && self.top_block_is_switch_body() {
                    self.in_case_body = true;
                    // Case bodies start on their own line, unless a block follows (`case 2: {`).
                    if next_kind != TokenKind::CurlyOpenBracketToken {
                        self.layout.break_line();
                    }
                }
                self.case_colon_pending = false;
            }
            TokenKind::CaseToken | TokenKind::DefaultToken => {
                self.case_colon_pending = true;
            }
            _ => {}
        }

        self.update_prev_tracking(token);
    }

    /// `{}` on one line: blank/indent decisions for the opener, both brace glyphs, then the
    /// closer's trailing comments and bookkeeping (its leading trivia is empty by contract).
    fn print_collapsed_empty_block(&mut self, i: usize, tokens: &[SyntaxToken]) {
        let open = &tokens[i];
        let close = &tokens[i + 1];
        let space_before = !self.layout.at_line_start()
            && needs_space(
                self.prev_prev_kind,
                self.prev_kind.unwrap_or(TokenKind::EndOfFileToken),
                open.kind,
            );

        self.set_anchor_offset(open);
        self.insert_blank_lines(open.kind);
        self.emit_leading_trivia(open);

        if self.layout.at_line_start() {
            self.layout.write_indent(self.current_indent(open.kind));
        } else if space_before {
            self.layout.space();
        }
        self.layout.text("{}");
        self.emit_trailing_trivia(close);
        if self.pending_switch {
            self.pending_switch = false;
        }
        // Mirror the non-collapsed `}` post-emission break (join rules included).
        let next_kind = tokens
            .get(i + 2)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::EndOfFileToken);
        if !matches!(
            next_kind,
            TokenKind::ElseToken
                | TokenKind::WhileToken
                | TokenKind::SemicolonToken
                | TokenKind::CommaToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
                | TokenKind::DotToken
        ) {
            self.layout.break_line();
        }
        self.prev_prev_kind = self.prev_kind;
        self.prev_kind = Some(TokenKind::CurlyCloseBracketToken);
        self.prev_end_line = self
            .line_index
            .line_of(close.position.end.saturating_sub(1));
    }

    /// Preserves up to one user blank line between statements and enforces exactly one blank
    /// line between top-level declarations that were adjacent in the source.
    fn insert_blank_lines(&mut self, kind: TokenKind) {
        if self.layout.is_empty()
            || matches!(kind, TokenKind::CurlyCloseBracketToken)
            || matches!(
                self.ctxs.last(),
                Some(Ctx::Paren) | Some(Ctx::Bracket) | Some(Ctx::TypeArgs)
            )
            || self.layout.has_pending_blank()
        {
            return;
        }
        if self.blank_gap() >= 1 {
            // Blank lines right after an opening brace are noise; strip them.
            if self.prev_kind != Some(TokenKind::CurlyOpenBracketToken) {
                self.layout.blank_line();
            }
        } else if self.brace_indent == 0
            && matches!(
                self.prev_kind,
                Some(TokenKind::SemicolonToken) | Some(TokenKind::CurlyCloseBracketToken)
            )
            && is_decl_starter(kind)
        {
            self.layout.blank_line();
        }
    }

    /// Source blank lines between the previous emitted token/comment and the current position.
    fn blank_gap(&self) -> usize {
        self.anchor_line().saturating_sub(self.prev_end_line + 1)
    }

    fn set_anchor_offset(&mut self, token: &SyntaxToken) {
        self.pending_anchor_offset = token
            .leading_trivia
            .first()
            .map(|t| t.position.start)
            .unwrap_or(token.position.start);
    }

    fn anchor_line(&self) -> usize {
        self.line_index.line_of(self.pending_anchor_offset)
    }

    fn current_indent(&self, kind: TokenKind) -> usize {
        let mut level = self.brace_indent;
        // The case-body bonus applies only to bare statements directly inside the switch
        // body; an explicit `{ ... }` block under a case indents on its own.
        if self.in_case_body
            && !matches!(kind, TokenKind::CaseToken | TokenKind::DefaultToken)
            && self.top_block_is_switch_body()
        {
            level += 1;
        }
        level
    }

    /// Indent for standalone comment lines — block level, without case-body bonus.
    pub(super) fn plain_indent(&self) -> usize {
        self.brace_indent
    }

    fn top_block_is_switch_body(&self) -> bool {
        matches!(self.ctxs.last(), Some(Ctx::Block { switch_body: true }))
    }

    fn update_prev_tracking(&mut self, token: &SyntaxToken) {
        self.prev_prev_kind = self.prev_kind;
        self.prev_kind = Some(token.kind);
        let mut end = token.position.end.saturating_sub(1);
        for t in &token.trailing_trivia {
            end = end.max(t.position.end.saturating_sub(1));
        }
        self.prev_end_line = self.line_index.line_of(end);
    }
}
