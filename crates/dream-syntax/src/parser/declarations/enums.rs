use super::super::Parser;
use crate::nodes::Visibility;
use crate::token::token_kind::TokenKind;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses an enum declaration. Two forms share this parser:
    /// - C-style integer enum: `enum Color { Red, Green = 5, Blue }`. Members without an explicit
    ///   value continue from the previous member's value (starting at 0).
    /// - Discriminated union: `enum Shape { Circle(float), Empty }`, optionally generic
    ///   `enum Option<T> { Some(T), None }`. A variant payload is positional (`T`) and/or named
    ///   (`name: Type`); the variant's `value` is its discriminant (sequential from 0).
    ///
    /// Methods may be declared in the body alongside variants (`public fun is_some(): bool { ... }`),
    /// using the same member classification as classes (look past `public`/`static`/`async` for `fun`).
    pub(crate) fn parse_enum_declaration(&mut self) -> crate::nodes::EnumDeclarationNode<'a> {
        let first_trivia = self.current_token().leading_trivia.clone();
        let attributes = self.parse_attributes();

        // A doc comment that preceded the first attribute (e.g. above `@json`) is consumed with the
        // attribute. Recover it so the comment still reaches the enum name token for hover/LSP.
        let doc_trivia = Self::recover_doc_trivia(first_trivia, &attributes);

        // Optional `public`/`internal` / `sealed` modifiers (any order) before the `enum` keyword.
        let mut is_sealed = false;
        let mut visibility = Visibility::Private;
        loop {
            if self.try_consume_visibility(&mut visibility) {
                continue;
            }
            match self.current_token().kind {
                TokenKind::SealedToken => {
                    self.match_token(TokenKind::SealedToken);
                    is_sealed = true;
                }
                TokenKind::StaticToken => {
                    self.diagnostics.report_error(
                        "'static' cannot modify an enum".to_string(),
                        Some(self.current_token().position),
                    );
                    self.match_token(TokenKind::StaticToken);
                }
                _ => break,
            }
        }

        self.match_token(TokenKind::EnumToken);
        let is_enum_struct = self.current_token().kind == TokenKind::StructToken;
        if is_enum_struct {
            self.match_token(TokenKind::StructToken);
        }
        let mut name = self.match_token(TokenKind::IdentifierToken);
        Self::splice_leading_trivia(&mut name, doc_trivia);

        let (generic_parameters, generic_constraints) = self.take_generic_params();

        self.match_token(TokenKind::CurlyOpenBracketToken);

        let mut variants = Vec::new();
        let mut methods = Vec::new();
        let mut next_value: i64 = 0;
        while self.current_token().kind != TokenKind::CurlyCloseBracketToken
            && self.current_token().kind != TokenKind::EndOfFileToken
        {
            let index_before = self.current_token_index;
            let gate = self.parse_cfg_gate();
            if self.cfg_gate_dangles(&gate) {
                continue;
            }
            let lengths = (variants.len(), methods.len());
            let member_attributes = self.parse_attributes();

            // Classify like class members: look past visibility/static/async for `fun` / accessors.
            let mut m = 0;
            while matches!(
                self.peek_token(m).kind,
                TokenKind::PublicToken
                    | TokenKind::InternalToken
                    | TokenKind::StaticToken
                    | TokenKind::AsyncToken
            ) {
                m += 1;
            }
            let core = self.peek_token(m);
            let is_accessor = core.kind == TokenKind::IdentifierToken
                && crate::nodes::function::AccessorKind::from_keyword(&core.text).is_some()
                && self.peek_token(m + 1).kind == TokenKind::IdentifierToken
                && self.peek_token(m + 2).kind == TokenKind::OpenParenthesisToken;
            let is_special = core.kind == TokenKind::IdentifierToken
                && crate::nodes::types::is_special_member_name(&core.text)
                && self.peek_token(m + 1).kind == TokenKind::OpenParenthesisToken;

            if core.kind == TokenKind::FunToken
                || core.kind == TokenKind::ExternToken
                || is_accessor
                || is_special
            {
                if is_special {
                    self.diagnostics.report_error(
                        "Enums cannot declare 'constructor' or 'del'".to_string(),
                        Some(core.position),
                    );
                }
                methods.push(self.parse_function(Some(member_attributes)));
            } else {
                let variant_name = self.match_token(TokenKind::IdentifierToken);

                // A payload `(name: Type, ...)` makes this a discriminated-union variant.
                let mut fields = Vec::new();
                if self.current_token().kind == TokenKind::OpenParenthesisToken {
                    self.match_token(TokenKind::OpenParenthesisToken);
                    fields = self.parse_delimited_list(TokenKind::CloseParenthesisToken, |p| {
                        p.parse_variant_field()
                    });
                    let mut pos_i = 0usize;
                    for f in &mut fields {
                        if f.name.text == "_" {
                            f.name.text = format!("_{pos_i}");
                            pos_i += 1;
                        }
                    }
                }

                // C-style explicit value (`Green = 5`); only meaningful for payload-less variants.
                let value = if self.current_token().kind == TokenKind::EqualToken {
                    if !fields.is_empty() {
                        self.diagnostics.report_error(
                            format!(
                                "variant '{}' carries a payload and cannot have an explicit value",
                                variant_name.text
                            ),
                            Some(self.current_token().position),
                        );
                    }
                    self.match_token(TokenKind::EqualToken);
                    let negative = self.current_token().kind == TokenKind::MinusToken;
                    if negative {
                        self.match_token(TokenKind::MinusToken);
                    }
                    let is_number = self.current_token().kind == TokenKind::NumberToken;
                    let num = self.match_token(TokenKind::NumberToken);
                    let parsed = crate::number::parse_int_literal(&num.text)
                        .map(|v| if negative { -v } else { v })
                        .and_then(|v| i32::try_from(v).ok());
                    match parsed {
                        Some(v) => v,
                        None => {
                            if is_number {
                                self.diagnostics.report_error(
                                    format!(
                                        "enum value '{}{}' must be an integer in the int range",
                                        if negative { "-" } else { "" },
                                        num.text
                                    ),
                                    Some(num.position),
                                );
                            }
                            0
                        }
                    }
                } else {
                    match i32::try_from(next_value) {
                        Ok(v) => v,
                        Err(_) => {
                            self.diagnostics.report_error(
                                format!(
                                    "enum value of '{}' overflows int; give it an explicit value",
                                    variant_name.text
                                ),
                                Some(variant_name.position),
                            );
                            0
                        }
                    }
                };
                next_value = i64::from(value) + 1;
                variants.push(crate::nodes::EnumVariantNode {
                    name: variant_name,
                    fields,
                    value,
                });

                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                }
            }
            if !self.keep_cfg_item(gate) {
                variants.truncate(lengths.0);
                methods.truncate(lengths.1);
            }
            // Safety: never spin on an unexpected token.
            if self.current_token_index == index_before {
                self.next_token();
            }
        }
        self.match_token(TokenKind::CurlyCloseBracketToken);
        let mut seen_values = std::collections::BTreeMap::new();
        for variant in &variants {
            if let Some(first) = seen_values.insert(variant.value, &variant.name.text) {
                self.diagnostics.report_error(
                    format!(
                        "enum variant '{}' has the same value {} as '{}'",
                        variant.name.text, variant.value, first
                    ),
                    Some(variant.name.position),
                );
            }
        }
        let name_pos = name.position;
        let name_text = name.text.clone();
        let mut decl =
            crate::nodes::EnumDeclarationNode::new(attributes, name, generic_parameters, variants);
        decl.generic_constraints = generic_constraints;
        decl.methods = methods;
        decl.is_sealed = is_sealed;
        decl.is_enum_struct = is_enum_struct;
        decl.visibility = visibility;
        if is_enum_struct && !decl.is_data_enum() {
            self.diagnostics.report_error(
                format!(
                    "'enum struct {}' must have at least one variant payload; use 'enum' for a C-style integer enum",
                    name_text
                ),
                Some(name_pos),
            );
        }
        decl
    }

    /// Parses a single discriminated-union variant payload field: `name: Type` or a positional
    /// `Type` (named `_0`, `_1`, … by the caller via empty name + later fill). Positional fields
    /// are a type with no preceding `name:`.
    pub(crate) fn parse_variant_field(&mut self) -> crate::nodes::struct_node::StructFieldNode {
        let positional = !(self.current_token().kind == TokenKind::IdentifierToken
            && self.peek_token(1).kind == TokenKind::ColonToken);
        let field_name = if positional {
            crate::token::syntax_token::SyntaxToken::new(
                TokenKind::IdentifierToken,
                self.current_token().position,
                "_".to_string(),
            )
        } else {
            let n = self.match_token(TokenKind::IdentifierToken);
            self.match_token(TokenKind::ColonToken);
            n
        };
        let type_position = self.current_token().position;
        let parsed_type = self.parse_type();
        let field_type_token = crate::token::syntax_token::SyntaxToken::new(
            TokenKind::IdentifierToken,
            type_position,
            parsed_type.get_type(),
        );
        crate::nodes::struct_node::StructFieldNode {
            attributes: Vec::new(),
            name: field_name,
            visibility: Visibility::Public,
            is_weak: false,
            is_unowned: false,
            type_token: field_type_token,
            field_type: parsed_type,
        }
    }
}
