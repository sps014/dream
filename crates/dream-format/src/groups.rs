use dream_syntax::token::{syntax_token::SyntaxToken, token_kind::TokenKind};

use crate::{
    generics::detect_type_arg_regions,
    spacing::{is_try_question_mark, needs_space},
};

/// Layout metadata is indexed by token so nested lists are measured once, independent of
/// their source line breaks. Braces remain hard breaks, including closures inside calls.
pub(super) struct Groups {
    pub closes: Vec<Option<usize>>,
    pub ternary_colons: Vec<bool>,
    widths: Vec<usize>,
    hard_breaks: Vec<usize>,
}

impl Groups {
    pub fn new(tokens: &[SyntaxToken]) -> Self {
        let mut closes = vec![None; tokens.len()];
        let mut stack = Vec::new();
        for (i, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::OpenParenthesisToken | TokenKind::OpenBracketToken => {
                    stack.push((i, token.kind));
                }
                TokenKind::CloseParenthesisToken | TokenKind::CloseBracketToken => {
                    if let Some(&(open, kind)) = stack.last()
                        && matches!(
                            (kind, token.kind),
                            (
                                TokenKind::OpenParenthesisToken,
                                TokenKind::CloseParenthesisToken
                            ) | (TokenKind::OpenBracketToken, TokenKind::CloseBracketToken)
                        )
                    {
                        stack.pop();
                        closes[open] = Some(i);
                    }
                }
                _ => {}
            }
        }
        let mut ternary_colons = vec![false; tokens.len()];
        let mut questions = Vec::new();
        let mut depth = 0usize;
        for (i, token) in tokens.iter().enumerate() {
            match token.kind {
                TokenKind::OpenParenthesisToken
                | TokenKind::OpenBracketToken
                | TokenKind::CurlyOpenBracketToken => depth += 1,
                TokenKind::CloseParenthesisToken
                | TokenKind::CloseBracketToken
                | TokenKind::CurlyCloseBracketToken => depth = depth.saturating_sub(1),
                TokenKind::QuestionMarkToken if !is_try_question_mark(tokens, i) => {
                    questions.push(depth)
                }
                TokenKind::ColonToken if questions.last() == Some(&depth) => {
                    questions.pop();
                    ternary_colons[i] = true;
                }
                _ => {}
            }
        }
        let mut type_delimiter = vec![false; tokens.len()];
        for (open, close) in detect_type_arg_regions(tokens) {
            type_delimiter[open] = true;
            type_delimiter[close] = true;
        }
        let mut widths = vec![0usize];
        let mut hard_breaks = vec![0usize];
        for (i, token) in tokens.iter().enumerate() {
            let space = i > 0
                && needs_space(
                    i.checked_sub(2).map(|j| tokens[j].kind),
                    tokens[i - 1].kind,
                    token.kind,
                )
                && !type_delimiter[i]
                && !(tokens[i - 1].kind == TokenKind::SmallerThanToken && type_delimiter[i - 1]);
            widths.push(widths[i] + token.text.chars().count() + usize::from(space));
            let hard = token.kind == TokenKind::CurlyOpenBracketToken
                || token.text.contains('\n')
                || !token.leading_trivia.is_empty()
                || !token.trailing_trivia.is_empty();
            hard_breaks.push(hard_breaks[i] + usize::from(hard));
        }
        Self {
            closes,
            ternary_colons,
            widths,
            hard_breaks,
        }
    }

    pub fn width(&self, start: usize, end: usize) -> usize {
        self.widths[end + 1] - self.widths[start]
    }

    pub fn must_break(&self, start: usize, end: usize) -> bool {
        self.hard_breaks[end + 1] != self.hard_breaks[start]
    }
}
