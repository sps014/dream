mod opaque;
mod policy;

use crate::MAX_WIDTH;
use crate::generics::detect_type_arg_regions;
use crate::groups::Groups;
use crate::layout::Layout;
use crate::line_index::LineIndex;
use crate::spacing::{ends_operand, is_decl_starter, is_try_question_mark, needs_space};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;

/// Delimiter nesting. `Block` drives indentation; `Paren`/`Bracket`/`TypeArgs` suppress
/// statement newlines (`;` inside a `for` header or subscript stays inline).
#[derive(Clone, Copy)]
enum Ctx {
    Block {
        switch_body: bool,
        case_body: bool,
        enum_body: bool,
        do_body: bool,
        type_body: bool,
        /// Whether the most recent member of a type body is separated by blank lines.
        member_spaced: bool,
    },
    Paren {
        multiline: bool,
        continuation: bool,
    },
    Bracket {
        multiline: bool,
        continuation: bool,
    },
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
    pending_enum: bool,
    pending_do: bool,
    pending_type: bool,
    chain_indent: bool,
    attribute_end: Option<usize>,
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
            pending_enum: false,
            pending_do: false,
            pending_type: false,
            chain_indent: false,
            attribute_end: None,
            case_colon_pending: false,
            prev_kind: None,
            prev_prev_kind: None,
            prev_end_line: 0,
            pending_anchor_offset: 0,
        }
    }

    pub fn run(mut self, source: &str, tokens: &[SyntaxToken]) -> String {
        let opaque = crate::opaque::regions(source);
        let mut next_opaque = 0;
        let regions = detect_type_arg_regions(tokens);
        let groups = Groups::new(tokens);
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

            if let Some(&(start, end)) = opaque.get(next_opaque)
                && token.position.start == start
            {
                let close = tokens
                    .partition_point(|token| token.position.end <= end)
                    .saturating_sub(1);
                self.print_opaque(source, i, close, end, tokens);
                i = close + 1;
                next_opaque += 1;
                while next_region < regions.len() && regions[next_region].0 < i {
                    next_region += 1;
                }
                continue;
            }

            if next_region < regions.len() && regions[next_region].0 == i {
                open_regions.push(regions[next_region]);
                self.ctxs.push(Ctx::TypeArgs);
                next_region += 1;
            }

            if token.kind == TokenKind::CurlyOpenBracketToken
                && tokens.get(i + 1).map(|t| t.kind) == Some(TokenKind::CurlyCloseBracketToken)
                && token.trailing_trivia.is_empty()
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

            self.print_token(i, tokens, &groups);

            while open_regions.last().map(|r| r.1) == Some(i) {
                open_regions.pop();
                self.ctxs.pop();
            }

            i += 1;
        }

        self.layout.finish()
    }

    fn print_token(&mut self, i: usize, tokens: &[SyntaxToken], groups: &Groups) {
        let token = &tokens[i];
        let kind = token.kind;
        let next_kind = tokens
            .get(i + 1)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::EndOfFileToken);

        // Ordering matters: blank-line decisions precede comment emission so a preserved blank
        // separates the previous statement from this token's leading comments — never the
        // comments from their own code.
        let closes_do = kind == TokenKind::CurlyCloseBracketToken
            && matches!(self.ctxs.last(), Some(Ctx::Block { do_body: true, .. }));
        self.set_anchor_offset(token);
        match kind {
            TokenKind::CurlyCloseBracketToken => {
                if let Some(Ctx::Block { .. }) = self.ctxs.last() {
                    self.ctxs.pop();
                    self.brace_indent = self.brace_indent.saturating_sub(1);
                }
                // Take the closing brace onto its own (dedented) line; whether it breaks
                // *after* — or joins `else`/`while` — is decided post-emission.
                self.layout.break_line();
            }
            TokenKind::CaseToken | TokenKind::DefaultToken => {
                if let Some(Ctx::Block { case_body, .. }) = self.ctxs.last_mut() {
                    *case_body = false;
                }
                self.case_colon_pending = false;
                self.layout.break_line();
            }
            _ => {}
        }

        let closing_multiline = matches!(
            kind,
            TokenKind::CloseParenthesisToken | TokenKind::CloseBracketToken
        ) && matches!(
            self.ctxs.last(),
            Some(
                Ctx::Paren {
                    multiline: true,
                    ..
                } | Ctx::Bracket {
                    multiline: true,
                    ..
                }
            )
        );
        if closing_multiline {
            self.close_list();
            self.layout.break_line();
        }
        if kind == TokenKind::AtToken && self.at_statement_level() {
            self.layout.break_line();
            let name_end = i + 1;
            self.attribute_end = Some(
                groups
                    .closes
                    .get(i + 2)
                    .copied()
                    .flatten()
                    .unwrap_or(name_end),
            );
        }
        self.separate_members(i, tokens);
        self.insert_blank_lines(kind);
        self.emit_leading_trivia(token);

        if kind == TokenKind::DotToken
            && Self::should_break_before_dot(self.prev_kind)
            && (self.chain_indent || self.chain_overflows(i, tokens, groups))
            && !self.layout.at_line_start()
        {
            self.layout.break_line();
            self.chain_indent = true;
        }

        let expression_operator = matches!(
            kind,
            TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::StarToken
                | TokenKind::SlashToken
                | TokenKind::AmpersandAmpersandToken
                | TokenKind::PipePipeToken
                | TokenKind::QuestionQuestionToken
                | TokenKind::EqualEqualToken
                | TokenKind::NotEqualToken
                | TokenKind::QuestionMarkToken
        ) && self.prev_kind.is_some_and(ends_operand)
            && !(kind == TokenKind::QuestionMarkToken && is_try_question_mark(tokens, i));
        if expression_operator
            && self.chain_overflows(i, tokens, groups)
            && !matches!(self.ctxs.last(), Some(Ctx::TypeArgs))
            && !self.layout.at_line_start()
        {
            self.layout.break_line();
            self.chain_indent = true;
        }

        if self.layout.at_line_start() {
            self.layout.write_indent(self.current_indent(kind));
        } else {
            let mut space = needs_space(
                self.prev_prev_kind,
                self.prev_kind.unwrap_or(TokenKind::EndOfFileToken),
                kind,
            );
            if kind == TokenKind::QuestionMarkToken && is_try_question_mark(tokens, i) {
                space = false;
            }
            if self.prev_kind == Some(TokenKind::QuestionMarkToken)
                && !is_try_question_mark(tokens, i - 1)
            {
                space = true;
            }
            if groups.ternary_colons[i] {
                space = true;
            }
            if kind == TokenKind::OpenParenthesisToken
                && self.prev_prev_kind == Some(TokenKind::OperatorToken)
            {
                space = false;
            }
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
                if !Self::joins_closer(next_kind, closes_do) {
                    self.layout.break_line();
                }
            }
            TokenKind::CurlyOpenBracketToken => {
                let switch_body = self.pending_switch;
                self.pending_switch = false;
                let enum_body = self.pending_enum;
                self.pending_enum = false;
                self.ctxs.push(Ctx::Block {
                    switch_body,
                    case_body: false,
                    enum_body,
                    do_body: self.pending_do,
                    type_body: self.pending_type && !enum_body,
                    member_spaced: false,
                });
                self.pending_do = false;
                self.pending_type = false;
                self.chain_indent = false;
                self.brace_indent += 1;
                self.layout.break_line();
            }
            TokenKind::ClassToken
            | TokenKind::StructToken
            | TokenKind::InterfaceToken
            | TokenKind::ExtendToken => {
                self.pending_type = true;
            }
            TokenKind::OpenParenthesisToken => {
                self.pending_type = false;
                self.open_list(i, tokens, groups, true);
            }
            TokenKind::CloseParenthesisToken => {
                if !closing_multiline && matches!(self.ctxs.last(), Some(Ctx::Paren { .. })) {
                    self.close_list();
                }
            }
            TokenKind::OpenBracketToken => {
                self.open_list(i, tokens, groups, false);
            }
            TokenKind::CloseBracketToken => {
                if !closing_multiline && matches!(self.ctxs.last(), Some(Ctx::Bracket { .. })) {
                    self.close_list();
                }
            }
            TokenKind::DoToken => {
                self.pending_do = true;
            }
            TokenKind::EnumToken => {
                self.pending_enum = true;
            }
            TokenKind::SwitchToken => {
                self.pending_switch = true;
            }
            TokenKind::SemicolonToken => {
                self.pending_switch = false;
                self.pending_type = false;
                self.chain_indent = false;
                if !matches!(
                    self.ctxs.last(),
                    Some(Ctx::Paren { .. }) | Some(Ctx::Bracket { .. }) | Some(Ctx::TypeArgs)
                ) {
                    self.layout.break_line();
                }
            }
            TokenKind::ColonToken => {
                if self.case_colon_pending && self.top_block_is_switch_body() {
                    if let Some(Ctx::Block { case_body, .. }) = self.ctxs.last_mut() {
                        *case_body = true;
                    }
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
            TokenKind::CommaToken => {
                self.chain_indent = false;
                if matches!(
                    self.ctxs.last(),
                    Some(
                        Ctx::Paren {
                            multiline: true,
                            ..
                        } | Ctx::Bracket {
                            multiline: true,
                            ..
                        } | Ctx::Block {
                            enum_body: true,
                            ..
                        } | Ctx::Block {
                            switch_body: true,
                            ..
                        }
                    )
                ) {
                    self.layout.break_line();
                }
            }
            _ => {}
        }

        if self.attribute_end == Some(i) {
            self.layout.break_line();
            self.attribute_end = None;
        }
        self.update_prev_tracking(token);
    }
}
