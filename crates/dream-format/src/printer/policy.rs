use super::*;

impl Printer {
    /// `{}` on one line: blank/indent decisions for the opener, both brace glyphs, then the
    /// closer's trailing comments and bookkeeping (its leading trivia is empty by contract).
    pub(super) fn print_collapsed_empty_block(&mut self, i: usize, tokens: &[SyntaxToken]) {
        let closes_do = self.pending_do;
        self.pending_do = false;
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
        if !Self::joins_closer(next_kind, closes_do) {
            self.layout.break_line();
        }
        self.prev_prev_kind = self.prev_kind;
        self.prev_kind = Some(TokenKind::CurlyCloseBracketToken);
        self.prev_end_line = self
            .line_index
            .line_of(close.position.end.saturating_sub(1));
    }

    pub(super) fn joins_closer(next: TokenKind, do_body: bool) -> bool {
        matches!(
            next,
            TokenKind::ElseToken
                | TokenKind::SemicolonToken
                | TokenKind::CommaToken
                | TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
                | TokenKind::DotToken
                | TokenKind::QuestionMarkToken
        ) || (next == TokenKind::WhileToken && do_body)
    }

    pub(super) fn insert_blank_lines(&mut self, kind: TokenKind) {
        if self.layout.is_empty()
            || matches!(kind, TokenKind::CurlyCloseBracketToken)
            || matches!(
                self.ctxs.last(),
                Some(Ctx::Paren { .. }) | Some(Ctx::Bracket { .. }) | Some(Ctx::TypeArgs)
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
        } else if (self.brace_indent == 0
            || (self.prev_kind == Some(TokenKind::CurlyCloseBracketToken)
                && !matches!(kind, TokenKind::LetToken | TokenKind::ConstToken)))
            && matches!(
                self.prev_kind,
                Some(TokenKind::SemicolonToken) | Some(TokenKind::CurlyCloseBracketToken)
            )
            && is_decl_starter(kind)
            && !matches!(
                kind,
                TokenKind::ImportToken | TokenKind::LetToken | TokenKind::ConstToken
            )
        {
            self.layout.blank_line();
        }
    }

    /// Source blank lines between the previous emitted token/comment and the current position.
    pub(super) fn blank_gap(&self) -> usize {
        self.anchor_line().saturating_sub(self.prev_end_line + 1)
    }

    pub(super) fn set_anchor_offset(&mut self, token: &SyntaxToken) {
        self.pending_anchor_offset = token
            .leading_trivia
            .first()
            .map(|t| t.position.start)
            .unwrap_or(token.position.start);
    }

    pub(super) fn anchor_line(&self) -> usize {
        self.line_index.line_of(self.pending_anchor_offset)
    }

    pub(super) fn current_indent(&self, kind: TokenKind) -> usize {
        let mut level = self.brace_indent;
        // The case-body bonus applies only to bare statements directly inside the switch
        // body; an explicit `{ ... }` block under a case indents on its own.
        if !matches!(
            kind,
            TokenKind::CaseToken | TokenKind::DefaultToken | TokenKind::CurlyCloseBracketToken
        ) && matches!(
            self.ctxs.last(),
            Some(Ctx::Block {
                case_body: true,
                ..
            })
        ) {
            level += 1;
        }
        // Continuation indent for a postfix chain that rustfmt would wrap: `foo()\n    .await`.
        if self.chain_indent && kind != TokenKind::CurlyCloseBracketToken {
            level += 1;
        }
        level += self
            .ctxs
            .iter()
            .filter(|ctx| {
                matches!(
                    ctx,
                    Ctx::Paren {
                        multiline: true,
                        ..
                    } | Ctx::Bracket {
                        multiline: true,
                        ..
                    }
                )
            })
            .count();
        level += self
            .ctxs
            .iter()
            .filter(|ctx| {
                matches!(
                    ctx,
                    Ctx::Paren {
                        continuation: true,
                        ..
                    } | Ctx::Bracket {
                        continuation: true,
                        ..
                    }
                )
            })
            .count();
        level
    }

    /// Rust-style chain wrap: break before `.await` / `.method(` when the receiver already ended
    /// (`foo().bar()`, `foo().await`, `foo().await?.ok()`).
    pub(super) fn should_break_before_dot(prev: Option<TokenKind>) -> bool {
        matches!(
            prev,
            Some(
                TokenKind::CloseParenthesisToken
                    | TokenKind::CloseBracketToken
                    | TokenKind::AwaitToken
                    | TokenKind::QuestionMarkToken
            )
        )
    }

    /// Indent for standalone comment lines — block level, without case-body bonus.
    pub(crate) fn plain_indent(&self) -> usize {
        self.brace_indent
    }

    pub(super) fn top_block_is_switch_body(&self) -> bool {
        matches!(
            self.ctxs.last(),
            Some(Ctx::Block {
                switch_body: true,
                ..
            })
        )
    }

    pub(super) fn at_statement_level(&self) -> bool {
        matches!(self.ctxs.last(), None | Some(Ctx::Block { .. }))
    }

    pub(super) fn open_list(
        &mut self,
        i: usize,
        tokens: &[SyntaxToken],
        groups: &Groups,
        paren: bool,
    ) {
        let multiline = groups.closes[i].is_some_and(|close| {
            if close == i + 1 {
                return false;
            }
            let control = paren
                && matches!(
                    self.prev_kind,
                    Some(
                        TokenKind::IfToken
                            | TokenKind::WhileToken
                            | TokenKind::ForToken
                            | TokenKind::SwitchToken
                            | TokenKind::LockToken
                    )
                );
            if control {
                return false;
            }
            let suffix = if self.at_statement_level()
                && tokens
                    .get(close + 1)
                    .is_some_and(|token| token.kind == TokenKind::ColonToken)
            {
                let end = tokens
                    .iter()
                    .enumerate()
                    .skip(close + 1)
                    .find(|(_, token)| {
                        matches!(
                            token.kind,
                            TokenKind::CurlyOpenBracketToken
                                | TokenKind::SemicolonToken
                                | TokenKind::EndOfFileToken
                        )
                    })
                    .map_or(tokens.len() - 1, |(index, _)| index);
                groups.width(close + 1, end)
            } else {
                tokens.get(close + 1).map_or(0, |t| t.text.chars().count())
            };
            groups.must_break(i + 1, close)
                || self.layout.column() + groups.width(i + 1, close) + suffix > MAX_WIDTH
        });
        let continuation = self.chain_indent;
        self.ctxs.push(if paren {
            Ctx::Paren {
                multiline,
                continuation,
            }
        } else {
            Ctx::Bracket {
                multiline,
                continuation,
            }
        });
        self.chain_indent = false;
        if multiline {
            self.layout.break_line();
        }
    }

    pub(super) fn close_list(&mut self) {
        if let Some(Ctx::Paren { continuation, .. } | Ctx::Bracket { continuation, .. }) =
            self.ctxs.pop()
        {
            self.chain_indent = continuation;
        }
    }

    pub(super) fn chain_overflows(
        &self,
        i: usize,
        tokens: &[SyntaxToken],
        groups: &Groups,
    ) -> bool {
        let mut depth = 0usize;
        let mut end = i;
        for (j, token) in tokens.iter().enumerate().skip(i) {
            match token.kind {
                TokenKind::OpenParenthesisToken | TokenKind::OpenBracketToken => depth += 1,
                TokenKind::CloseParenthesisToken | TokenKind::CloseBracketToken => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::CommaToken if depth == 0 => break,
                TokenKind::SemicolonToken
                | TokenKind::CurlyOpenBracketToken
                | TokenKind::CurlyCloseBracketToken
                | TokenKind::EndOfFileToken => break,
                _ => {}
            }
            end = j;
        }
        self.layout.column() + groups.width(i, end) > MAX_WIDTH
    }

    pub(super) fn update_prev_tracking(&mut self, token: &SyntaxToken) {
        self.prev_prev_kind = self.prev_kind;
        self.prev_kind = Some(token.kind);
        let mut end = token.position.end.saturating_sub(1);
        for t in &token.trailing_trivia {
            end = end.max(t.position.end.saturating_sub(1));
        }
        self.prev_end_line = self.line_index.line_of(end);
    }
}
