use super::super::Parser;
use crate::nodes::{ExpressionNode, PatternNode, SwitchArm, SwitchArmBody, Type};
use crate::token::syntax_token::SyntaxToken;
use crate::token::token_kind::TokenKind;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses the shared `switch (subject) {` header, returning the subject. Both the
    /// pattern-matching form ([`parse_switch_expr`]) and the C-style `case`/`default` form
    /// ([`parse_switch`](Self::parse_switch)) start here, then branch on the body.
    pub(crate) fn parse_switch_header(&mut self) -> (SyntaxToken, ExpressionNode<'a>) {
        let switch_tok = self.match_token(TokenKind::SwitchToken);
        self.match_token(TokenKind::OpenParenthesisToken);
        let subject = self.parse_expression(0);
        self.match_token(TokenKind::CloseParenthesisToken);
        self.match_token(TokenKind::CurlyOpenBracketToken);
        (switch_tok, subject)
    }

    /// Parses the pattern-matching arms `pattern [if guard] => body, ...` up to and including the
    /// closing `}`, assuming the `switch (...) {` header has already been consumed. Each arm body is
    /// either an expression (`=> expr`) or a statement block (`=> { ... }`); a trailing comma after
    /// an arm is optional.
    pub(crate) fn parse_switch_arms(&mut self) -> Vec<SwitchArm<'a>> {
        self.parse_delimited_list(TokenKind::CurlyCloseBracketToken, |p| {
            let pattern = p.parse_pattern();

            // Optional `if <guard>` after the pattern.
            let guard = if p.current_token().kind == TokenKind::IfToken {
                p.match_token(TokenKind::IfToken);
                Some(p.parse_expression(0))
            } else {
                None
            };

            p.match_token(TokenKind::FatArrowToken);

            let body = if p.current_token().kind == TokenKind::CurlyOpenBracketToken {
                SwitchArmBody::Block(p.parse_block())
            } else {
                SwitchArmBody::Expr(p.parse_expression(0))
            };

            SwitchArm {
                pattern,
                guard,
                body,
            }
        })
    }

    /// Parses a `switch (subject) { pattern [if guard] => body, ... }` expression (the
    /// pattern-matching form). The C-style `case`/`default` form is a statement and is parsed by
    /// [`parse_switch`](Self::parse_switch).
    pub(crate) fn parse_switch_expr(&mut self) -> ExpressionNode<'a> {
        let (switch_tok, subject) = self.parse_switch_header();
        let arms = self.parse_switch_arms();
        ExpressionNode::Switch(switch_tok, self.arena.alloc(subject), arms)
    }

    /// Parses a match pattern, including a top-level or-pattern (`pat | pat | ...`): one or more
    /// [`Self::parse_pattern_atom`]s separated by `|`, collapsing to a single [`PatternNode::Or`]
    /// only when there is more than one alternative.
    pub(crate) fn parse_pattern(&mut self) -> PatternNode {
        if !self.enter_nesting() {
            let span = self.current_token().position;
            return PatternNode::Wildcard(SyntaxToken::new(
                TokenKind::IdentifierToken,
                span,
                "_".to_string(),
            ));
        }
        let result = crate::stack::grow(|| self.parse_pattern_limited());
        self.leave_nesting();
        result
    }

    fn parse_pattern_limited(&mut self) -> PatternNode {
        let first = self.parse_pattern_atom();
        if self.current_token().kind != TokenKind::BitWisePipeToken {
            return first;
        }
        let mut alts = vec![first];
        while self.current_token().kind == TokenKind::BitWisePipeToken {
            self.match_token(TokenKind::BitWisePipeToken);
            alts.push(self.parse_pattern_atom());
        }
        PatternNode::Or(alts)
    }

    /// Parses a single pattern "atom" (one or-pattern alternative): `_` (wildcard), a literal or
    /// inclusive range literal (`1..5`), a bare identifier (a binding, later reinterpreted as a
    /// unit variant by the analyzer when it names one), or a variant pattern `Variant(sub, ...)` /
    /// `Enum.Variant(sub, ...)`.
    pub(crate) fn parse_pattern_atom(&mut self) -> PatternNode {
        let cur = self.current_token();
        match cur.kind {
            TokenKind::OpenParenthesisToken => self.parse_tuple_pattern(),
            TokenKind::IdentifierToken => {
                if cur.text == "_" {
                    let tok = self.next_token();
                    return PatternNode::Wildcard(tok);
                }
                let first = self.match_token(TokenKind::IdentifierToken);
                // `Enum.Variant[(...)]` - a qualified variant pattern.
                if self.current_token().kind == TokenKind::DotToken {
                    self.match_token(TokenKind::DotToken);
                    let variant = self.match_token(TokenKind::IdentifierToken);
                    let subs = self.parse_pattern_args();
                    return PatternNode::Variant(Some(first), variant, subs);
                }
                // `Variant(...)` - an unqualified variant pattern with a payload.
                if self.current_token().kind == TokenKind::OpenParenthesisToken {
                    let subs = self.parse_pattern_args();
                    return PatternNode::Variant(None, first, subs);
                }
                // A bare identifier: a binding (or a unit variant, resolved during analysis).
                PatternNode::Binding(first)
            }
            _ => {
                let lo = self.parse_literal_pattern();
                if self.current_token().kind == TokenKind::DotDotToken {
                    self.match_token(TokenKind::DotDotToken);
                    let hi = self.parse_literal_pattern();
                    return PatternNode::Range(lo, hi);
                }
                PatternNode::Literal(lo)
            }
        }
    }

    /// `(p0, p1, …)` as a tuple pattern. A single parenthesized pattern `(p)` is grouping, not a
    /// 1-tuple (Dream tuples have arity ≥ 2).
    pub(crate) fn parse_tuple_pattern(&mut self) -> PatternNode {
        self.match_token(TokenKind::OpenParenthesisToken);
        let elems =
            self.parse_delimited_list(TokenKind::CloseParenthesisToken, |p| p.parse_pattern());
        match elems.len() {
            0 => {
                self.diagnostics.report_error(
                    "tuple pattern requires at least two elements".to_string(),
                    Some(self.current_token().position),
                );
                PatternNode::Wildcard(self.current_token().clone())
            }
            1 => {
                let mut elems = elems;
                elems.remove(0)
            }
            _ => PatternNode::Tuple(elems),
        }
    }

    /// Parses the parenthesized sub-pattern list of a variant pattern, e.g. the `(x, None)` in
    /// `Pair(x, None)`. Returns an empty list when there is no `(...)`.
    pub(crate) fn parse_pattern_args(&mut self) -> Vec<PatternNode> {
        let mut subs = Vec::new();
        if self.current_token().kind == TokenKind::OpenParenthesisToken {
            self.match_token(TokenKind::OpenParenthesisToken);
            subs =
                self.parse_delimited_list(TokenKind::CloseParenthesisToken, |p| p.parse_pattern());
        }
        subs
    }

    /// Classifies a `NumberToken` into its concrete numeric [`Type`], stripping any type suffix
    /// from the token's text so downstream stages see only the numeric value. Recognized suffixes
    /// (case-insensitive): `h` (float16), `f` (float), `d` (double), `L` (long), `u` (uint),
    /// `uL`/`Lu` (ulong), `b` (byte). A bare literal with a decimal point is `float`, otherwise
    /// `int`.
    pub(crate) fn classify_number_literal(mut token: SyntaxToken) -> Type {
        let text = token.text.clone();
        let Some((body, suffix)) = crate::number::split_numeric_literal(&text) else {
            return Type::Integer(token);
        };
        token.text = body.to_string();
        match suffix.to_ascii_lowercase().as_str() {
            "b" => Type::Byte(token),
            "sb" => Type::SByte(token),
            "s" => Type::Short(token),
            "us" => Type::UShort(token),
            "ul" | "lu" => Type::ULong(token),
            "l" => Type::Long(token),
            "u" => Type::UInt(token),
            "d" => Type::Double(token),
            "f" => Type::Float(token),
            "h" => Type::Float16(token),
            _ => {
                if crate::number::numeric_body_is_float(body) {
                    Type::Float(token)
                } else {
                    Type::Integer(token)
                }
            }
        }
    }

    /// `-` applied directly to a decimal literal, as one negative literal, so the minimum of each
    /// signed type (`-2147483648`) is typed by its full value. A radix literal keeps the unary
    /// minus because its digits spell a bit pattern.
    pub(crate) fn negated_decimal_literal(
        operator: &SyntaxToken,
        operand: &ExpressionNode<'_>,
    ) -> Option<Type> {
        if operator.kind != TokenKind::MinusToken {
            return None;
        }
        let ExpressionNode::Literal(literal) = operand else {
            return None;
        };
        let negate = |token: &SyntaxToken| {
            if token.text.starts_with('-') || crate::number::is_radix_literal(&token.text) {
                return None;
            }
            let mut negated = token.clone();
            negated.text = format!("-{}", token.text);
            negated.position.start = operator.position.start;
            negated.position.col_no = operator.position.col_no;
            Some(negated)
        };
        Some(match literal {
            Type::Integer(t) => Type::Integer(negate(t)?),
            Type::Long(t) => Type::Long(negate(t)?),
            Type::SByte(t) => Type::SByte(negate(t)?),
            Type::Short(t) => Type::Short(negate(t)?),
            Type::Float16(t) => Type::Float16(negate(t)?),
            Type::Float(t) => Type::Float(negate(t)?),
            Type::Double(t) => Type::Double(negate(t)?),
            _ => return None,
        })
    }

    /// Parses a literal used as a pattern (`0`, `-5`, `3.14`, `"s"`, `'c'`, `true`, `Option.None`).
    /// Also reused to parse constant default parameter values.
    pub(crate) fn parse_literal_pattern(&mut self) -> Type {
        let cur = self.current_token();
        match cur.kind {
            TokenKind::IdentifierToken => {
                let first = self.match_token(TokenKind::IdentifierToken);
                if self.current_token().kind != TokenKind::DotToken {
                    self.diagnostics.report_error(
                        format!(
                            "Expected a pattern but found {}",
                            first.kind.friendly_name()
                        ),
                        Some(first.position),
                    );
                    return Type::Unknown;
                }
                self.match_token(TokenKind::DotToken);
                let second = self.match_token(TokenKind::IdentifierToken);
                Type::Struct(first, Some(vec![Type::Struct(second, None)]))
            }
            TokenKind::BooleanToken => Type::Boolean(self.match_token(TokenKind::BooleanToken)),
            TokenKind::StringToken => Type::String(self.match_token(TokenKind::StringToken)),
            TokenKind::CharToken => {
                let tok = self.next_token();
                let value = self.char_literal_value(&tok);
                let char_token =
                    SyntaxToken::new(TokenKind::CharToken, tok.position, value.to_string());
                Type::Char(char_token)
            }
            TokenKind::MinusToken | TokenKind::NumberToken => {
                let negative = cur.kind == TokenKind::MinusToken;
                if negative {
                    self.match_token(TokenKind::MinusToken);
                }
                let token = self.match_token(TokenKind::NumberToken);
                let mut classified = Self::classify_number_literal(token);
                if negative {
                    // Prepend the sign to the (suffix-stripped) numeric text of the literal.
                    classified = match classified {
                        Type::Integer(mut t) => {
                            t.text = format!("-{}", t.text);
                            Type::Integer(t)
                        }
                        Type::Long(mut t) => {
                            t.text = format!("-{}", t.text);
                            Type::Long(t)
                        }
                        Type::Float(mut t) => {
                            t.text = format!("-{}", t.text);
                            Type::Float(t)
                        }
                        Type::Float16(mut t) => {
                            t.text = format!("-{}", t.text);
                            Type::Float16(t)
                        }
                        Type::Double(mut t) => {
                            t.text = format!("-{}", t.text);
                            Type::Double(t)
                        }
                        other => other,
                    };
                }
                classified
            }
            _ => {
                self.diagnostics.report_error(
                    format!("Expected a pattern but found {}", cur.kind.friendly_name()),
                    Some(cur.position),
                );
                self.next_token();
                Type::Unknown
            }
        }
    }
}
