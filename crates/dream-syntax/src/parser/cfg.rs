use super::Parser;
use crate::conditional::{CFG_ATTRIBUTE, TARGET_NAMESPACE, TargetSymbol};
use crate::nodes::Type;
use crate::nodes::program::SourceNodeSpan;
use crate::token::token_kind::TokenKind;
use dream_text::text_span::TextSpan;
use std::collections::{BTreeMap, HashMap};

/// A leading `@cfg(condition)` consumed before the item it gates.
pub(super) struct CfgGate {
    start: TextSpan,
    /// Parser side tables saved before an inactive item is parsed, restored once it is dropped.
    inactive: Option<SavedTables>,
}

struct SavedTables {
    node_spans: BTreeMap<(bool, &'static str, Option<usize>, usize), SourceNodeSpan>,
    type_aliases: HashMap<String, Type>,
}

impl<'a, 'b> Parser<'a, 'b> {
    /// True when the cursor is on `@cfg`.
    pub(super) fn at_cfg_attribute(&self) -> bool {
        self.current_token().kind == TokenKind::AtToken && {
            let name = self.peek_token(1);
            name.kind == TokenKind::IdentifierToken && name.text == CFG_ATTRIBUTE
        }
    }

    /// Consumes a leading `@cfg(condition)`. Returns `None` when the cursor is not on one. A
    /// malformed condition is reported and keeps the item, so its names still resolve.
    pub(super) fn parse_cfg_gate(&mut self) -> Option<CfgGate> {
        if !self.at_cfg_attribute() {
            return None;
        }
        let at = self.next_token();
        self.next_token();
        let active = self.parse_cfg_condition().unwrap_or(true);

        while self.at_cfg_attribute() {
            self.diagnostics.report_error(
                "an item can have only one '@cfg'; combine the conditions with '&&' or '||'"
                    .to_string(),
                Some(self.current_token().position),
            );
            self.next_token();
            self.next_token();
            self.parse_cfg_condition();
        }

        let next = self.current_token();
        if matches!(
            next.kind,
            TokenKind::CurlyCloseBracketToken | TokenKind::EndOfFileToken
        ) {
            self.diagnostics.report_error(
                "'@cfg' must be followed by the declaration, member, or statement it controls"
                    .to_string(),
                Some(at.position),
            );
        } else if !at.leading_trivia.is_empty() {
            // Doc comments above `@cfg` document the gated item.
            let index = self.current_token_index;
            if let Some(token) = self.tokens.get_mut(index) {
                token.leading_trivia.splice(0..0, at.leading_trivia);
            }
        }

        let inactive = (!active).then(|| SavedTables {
            node_spans: self.node_spans.clone(),
            type_aliases: self.type_aliases.clone(),
        });
        Some(CfgGate {
            start: at.position,
            inactive,
        })
    }

    /// True when a gate was just consumed with nothing left to gate (already reported).
    pub(super) fn cfg_gate_dangles(&self, gate: &Option<CfgGate>) -> bool {
        gate.is_some()
            && matches!(
                self.current_token().kind,
                TokenKind::CurlyCloseBracketToken | TokenKind::EndOfFileToken
            )
    }

    /// Settles a gate once its item is parsed. Returns false when the item must be dropped; the
    /// parser then forgets everything the item recorded and remembers its range.
    pub(super) fn keep_cfg_item(&mut self, gate: Option<CfgGate>) -> bool {
        let Some(CfgGate {
            start,
            inactive: Some(saved),
        }) = gate
        else {
            return true;
        };
        self.node_spans = saved.node_spans;
        self.type_aliases = saved.type_aliases;
        let mut span = start;
        if let Some(last) = self
            .current_token_index
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
        {
            span.end = last.position.end.max(span.start);
        }
        self.inactive_spans.push(span);
        false
    }

    /// Reports `@cfg` found after another attribute, where it cannot gate the item.
    pub(super) fn report_misplaced_cfg(&mut self, at: TextSpan) {
        self.diagnostics.report_error(
            "'@cfg' must be the first attribute of a declaration, member, or statement".to_string(),
            Some(at),
        );
        self.parse_cfg_condition();
    }

    /// Parses `( condition )`. `None` means the condition was malformed and has been reported.
    fn parse_cfg_condition(&mut self) -> Option<bool> {
        if self.current_token().kind != TokenKind::OpenParenthesisToken {
            self.diagnostics.report_error(
                "'@cfg' needs a condition, like '@cfg(Target.Windows)'".to_string(),
                Some(self.current_token().position),
            );
            return None;
        }
        self.next_token();
        let value = self.parse_cfg_or();
        if self.current_token().kind != TokenKind::CloseParenthesisToken {
            self.diagnostics.report_error(
                format!(
                    "expected '&&', '||', or ')' in a '@cfg' condition but found {}",
                    self.current_token().kind.friendly_name()
                ),
                Some(self.current_token().position),
            );
            self.skip_to_cfg_close();
            return None;
        }
        self.next_token();
        value
    }

    fn parse_cfg_or(&mut self) -> Option<bool> {
        let mut value = self.parse_cfg_and();
        while self.current_token().kind == TokenKind::PipePipeToken {
            let iter = self.current_token_index;
            self.next_token();
            let right = self.parse_cfg_and();
            value = value.zip(right).map(|(l, r)| l || r);
            self.ensure_progress(iter);
        }
        value
    }

    fn parse_cfg_and(&mut self) -> Option<bool> {
        let mut value = self.parse_cfg_unary();
        while self.current_token().kind == TokenKind::AmpersandAmpersandToken {
            let iter = self.current_token_index;
            self.next_token();
            let right = self.parse_cfg_unary();
            value = value.zip(right).map(|(l, r)| l && r);
            self.ensure_progress(iter);
        }
        value
    }

    fn parse_cfg_unary(&mut self) -> Option<bool> {
        if !self.enter_nesting() {
            return None;
        }
        let value = match self.current_token().kind {
            TokenKind::BangToken => {
                self.next_token();
                self.parse_cfg_unary().map(|v| !v)
            }
            TokenKind::OpenParenthesisToken => {
                self.next_token();
                let value = self.parse_cfg_or();
                if self.current_token().kind == TokenKind::CloseParenthesisToken {
                    self.next_token();
                    value
                } else {
                    self.match_token(TokenKind::CloseParenthesisToken);
                    None
                }
            }
            _ => self.parse_cfg_symbol(),
        };
        self.leave_nesting();
        value
    }

    fn parse_cfg_symbol(&mut self) -> Option<bool> {
        let namespace = self.current_token();
        if namespace.kind != TokenKind::IdentifierToken {
            self.diagnostics.report_error(
                format!(
                    "expected a condition like 'Target.Windows' but found {}",
                    namespace.kind.friendly_name()
                ),
                Some(namespace.position),
            );
            if !matches!(
                namespace.kind,
                TokenKind::CloseParenthesisToken | TokenKind::EndOfFileToken
            ) {
                self.next_token();
            }
            return None;
        }
        self.next_token();
        if namespace.text != TARGET_NAMESPACE {
            self.diagnostics.report_error(
                format!(
                    "'@cfg' conditions test '{TARGET_NAMESPACE}.<name>', but found '{}'",
                    namespace.text
                ),
                Some(namespace.position),
            );
            if self.current_token().kind == TokenKind::DotToken {
                self.next_token();
                self.match_token(TokenKind::IdentifierToken);
            }
            return None;
        }
        self.match_token(TokenKind::DotToken);
        let name = self.match_token(TokenKind::IdentifierToken);
        if name.text.is_empty() {
            return None;
        }
        match TargetSymbol::from_name(&name.text) {
            Some(symbol) => Some(self.cfg.defines(symbol)),
            None => {
                let known = TargetSymbol::ALL
                    .iter()
                    .map(|symbol| format!("{TARGET_NAMESPACE}.{}", symbol.name()))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.diagnostics.report_error(
                    format!(
                        "unknown target '{TARGET_NAMESPACE}.{}'; expected one of {known}",
                        name.text
                    ),
                    Some(name.position),
                );
                None
            }
        }
    }

    /// Skips to the `)` closing a malformed condition, keeping nested parentheses balanced.
    fn skip_to_cfg_close(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.current_token().kind {
                TokenKind::EndOfFileToken => return,
                TokenKind::OpenParenthesisToken => depth += 1,
                TokenKind::CloseParenthesisToken if depth == 0 => {
                    self.next_token();
                    return;
                }
                TokenKind::CloseParenthesisToken => depth -= 1,
                TokenKind::CurlyOpenBracketToken | TokenKind::CurlyCloseBracketToken
                    if depth == 0 =>
                {
                    return;
                }
                _ => {}
            }
            self.next_token();
        }
    }
}
