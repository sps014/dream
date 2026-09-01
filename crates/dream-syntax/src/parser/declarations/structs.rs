use super::super::Parser;
use crate::nodes::Visibility;
use crate::token::token_kind::TokenKind;
use std::io::Error;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses a struct declaration
    pub(crate) fn parse_struct_declaration(
        &mut self,
    ) -> Result<crate::nodes::struct_node::StructDeclarationNode<'a>, Error> {
        let first_trivia = self.current_token().leading_trivia.clone();

        let attributes = self.parse_attributes();

        // `public`/`internal`, `sealed`, and `static` may appear in either order before the
        // `class`/`struct` keyword.
        let mut visibility = Visibility::Private;
        let mut is_sealed = false;
        let mut is_static = false;
        let mut is_shared = false;
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
                    self.match_token(TokenKind::StaticToken);
                    is_static = true;
                }
                TokenKind::SharedToken => {
                    self.match_token(TokenKind::SharedToken);
                    is_shared = true;
                }
                _ => break,
            }
        }

        // `ref struct` is a stack-only value type: in addition to being a value type (inline, copy
        // semantics), the compiler rejects any use of it that would let it escape the current stack
        // frame (stored in a heap object, used as a generic type argument, captured by a closure, or
        // crossing an `async` boundary — see `Analyzer::check_ref_struct_escapes`).
        let is_ref_struct = self.current_token().kind == TokenKind::RefToken;
        if is_ref_struct {
            self.match_token(TokenKind::RefToken);
        }

        // A value type is introduced with `struct`; a reference type with `class`. Both share the
        // same declaration shape and AST node, differing only in the `is_value` flag.
        let is_value = is_ref_struct || self.current_token().kind == TokenKind::StructToken;
        if self.current_token().kind == TokenKind::ClassToken {
            if is_ref_struct {
                self.diagnostics.report_error(
                    "'ref' may only precede 'struct', not 'class'".to_string(),
                    Some(self.current_token().position),
                );
            }
            self.match_token(TokenKind::ClassToken);
        } else {
            if is_static {
                self.diagnostics.report_error(
                    "'static' cannot modify a struct; use 'static class'".to_string(),
                    Some(self.current_token().position),
                );
            }
            self.match_token(TokenKind::StructToken);
        }
        let mut struct_name = self.match_token(TokenKind::IdentifierToken);
        Self::splice_leading_trivia(&mut struct_name, first_trivia);

        let (generic_parameters, generic_constraints) = self.take_generic_params();

        let mut primary_fields = Vec::new();
        let had_primary = self.current_token().kind == TokenKind::OpenParenthesisToken;
        if had_primary {
            self.match_token(TokenKind::OpenParenthesisToken);
            primary_fields = self.parse_delimited_list(TokenKind::CloseParenthesisToken, |p| {
                p.parse_primary_ctor_field()
            })?;
        }

        // Optional `: Iface1, Container<int>, ...` implements clause. Each entry is a (possibly
        // generic) interface type the class declares it satisfies; the class must provide a matching
        // method for every interface method (validated during semantic analysis).
        let mut implements = Vec::new();
        if self.current_token().kind == TokenKind::ColonToken {
            self.match_token(TokenKind::ColonToken);
            loop {
                let iter = self.current_token_index;
                implements.push(self.parse_type()?);
                if self.current_token().kind == TokenKind::CommaToken {
                    self.match_token(TokenKind::CommaToken);
                } else {
                    break;
                }
                self.ensure_progress(iter);
            }
        }

        let mut fields = primary_fields.clone();
        let mut methods = Vec::new();
        if self.current_token().kind == TokenKind::SemicolonToken {
            self.match_token(TokenKind::SemicolonToken);
        } else {
            self.match_token(TokenKind::CurlyOpenBracketToken);

            while self.current_token().kind != TokenKind::CurlyCloseBracketToken
                && self.current_token().kind != TokenKind::EndOfFileToken
            {
                let iter = self.current_token_index;
                let field_attributes = self.parse_attributes();

                // Classify the member by looking past any leading `public`/`static`/`async`: a
                // method (`fun`, `static fun`, `constructor`/`del`, `extern fun`) is dispatched to
                // `parse_function` (which consumes its own modifiers), otherwise it is a field.
                let mut m = 0;
                while matches!(
                    self.peek_token(m).kind,
                    TokenKind::PublicToken
                    | TokenKind::InternalToken
                    | TokenKind::StaticToken
                    | TokenKind::AsyncToken
                    // Receiver-mode qualifiers (`[borrow | unique] fun ...`) also precede the
                    // member's core token.
                    | TokenKind::OverrideToken
                    | TokenKind::BorrowToken
                    | TokenKind::UniqueToken
                ) {
                    m += 1;
                }
                let core = self.peek_token(m);
                let is_ctor_dtor = core.kind == TokenKind::IdentifierToken
                    && crate::nodes::types::is_special_member_name(&core.text)
                    && self.peek_token(m + 1).kind == TokenKind::OpenParenthesisToken;

                // TypeScript-style property accessor: `get name(...)` / `set name(...)`. `get`/`set`
                // are contextual keywords (still ordinary identifiers/field names elsewhere), so this
                // only binds when the next token is a property name followed by a parameter list.
                let is_accessor = core.kind == TokenKind::IdentifierToken
                    && crate::nodes::function::AccessorKind::from_keyword(&core.text).is_some()
                    && self.peek_token(m + 1).kind == TokenKind::IdentifierToken
                    && self.peek_token(m + 2).kind == TokenKind::OpenParenthesisToken;
                if core.kind == TokenKind::FunToken
                    || core.kind == TokenKind::ExternToken
                    || is_ctor_dtor
                    || is_accessor
                {
                    methods.push(self.parse_function(Some(field_attributes))?);
                } else {
                    // Fields are private by default; an explicit `public`/`internal` exposes them.
                    // `weak`/`unowned` are storage qualifiers that break ARC reference cycles (see
                    // `docs/language/memory.md`); any order/combination with the visibility modifier is
                    // accepted here, with `weak`+`unowned` together rejected during semantic analysis.
                    //
                    // Doc comments attach to the first token of the field (`public`, or the name when
                    // bare). Capture before consuming modifiers and splice onto the name for LSP.
                    let first_trivia = self.current_token().leading_trivia.clone();
                    let doc_trivia = Self::recover_doc_trivia(first_trivia, &field_attributes);
                    let mut field_visibility = Visibility::Private;
                    let mut field_weak = false;
                    let mut field_unowned = false;
                    loop {
                        if self.try_consume_visibility(&mut field_visibility) {
                            continue;
                        }
                        match self.current_token().kind {
                            TokenKind::WeakToken => {
                                self.match_token(TokenKind::WeakToken);
                                field_weak = true;
                            }
                            TokenKind::UnownedToken => {
                                self.match_token(TokenKind::UnownedToken);
                                field_unowned = true;
                            }
                            _ => break,
                        }
                    }
                    let mut field_name = self.match_token(TokenKind::IdentifierToken);
                    // Bare `/// doc\n name: T` already carries trivia on the identifier; only splice
                    // when a modifier/attribute ate it (`public name`, `@loc name`).
                    if field_name.leading_trivia.is_empty() {
                        Self::splice_leading_trivia(&mut field_name, doc_trivia);
                    }
                    self.match_token(TokenKind::ColonToken);

                    // Parse the full type (supporting generic args like `Map<string, JsonValue>`
                    // and arrays) and store its canonical spelling on the field.
                    let type_position = self.current_token().position;
                    let parsed_type = self.parse_type()?;
                    let field_type_token = crate::token::syntax_token::SyntaxToken::new(
                        TokenKind::IdentifierToken,
                        type_position,
                        parsed_type.get_type(),
                    );

                    self.match_token(TokenKind::SemicolonToken);
                    fields.push(crate::nodes::struct_node::StructFieldNode {
                        attributes: field_attributes,
                        name: field_name,
                        visibility: field_visibility,
                        is_weak: field_weak,
                        is_unowned: field_unowned,
                        type_token: field_type_token,
                        field_type: parsed_type,
                    });
                }
                self.ensure_progress(iter);
            }

            self.match_token(TokenKind::CurlyCloseBracketToken);
        }

        if had_primary {
            methods.insert(
                0,
                self.synthesize_primary_init(&primary_fields, &struct_name),
            );
        }
        if is_shared && is_value {
            self.diagnostics.report_error(
                "'shared' cannot modify a struct; use 'shared class'".to_string(),
                Some(struct_name.position),
            );
        }
        let mut decl = crate::nodes::struct_node::StructDeclarationNode::new(
            attributes,
            struct_name,
            generic_parameters,
            fields,
            methods,
            visibility,
        );
        decl.implements = implements;
        decl.is_value = is_value;
        decl.is_ref_struct = is_ref_struct;
        // Implicitly sealed so there is no instance surface to inherit; static `extend` helpers
        // are still allowed (see analyzer `register_extensions`).
        decl.is_sealed = is_sealed || is_static;
        decl.is_static = is_static;
        decl.is_shared = is_shared;
        decl.generic_constraints = generic_constraints;
        Ok(decl)
    }

    fn parse_primary_ctor_field(
        &mut self,
    ) -> Result<crate::nodes::struct_node::StructFieldNode, Error> {
        let mut field_visibility = Visibility::Private;
        self.try_consume_visibility(&mut field_visibility);
        let field_name = self.match_token(TokenKind::IdentifierToken);
        self.match_token(TokenKind::ColonToken);
        let type_position = self.current_token().position;
        let parsed_type = self.parse_type()?;
        let field_type_token = crate::token::syntax_token::SyntaxToken::new(
            TokenKind::IdentifierToken,
            type_position,
            parsed_type.get_type(),
        );
        Ok(crate::nodes::struct_node::StructFieldNode {
            attributes: Vec::new(),
            name: field_name,
            visibility: field_visibility,
            is_weak: false,
            is_unowned: false,
            type_token: field_type_token,
            field_type: parsed_type,
        })
    }

    fn synthesize_primary_init(
        &mut self,
        fields: &[crate::nodes::struct_node::StructFieldNode],
        struct_name: &crate::token::syntax_token::SyntaxToken,
    ) -> crate::nodes::function::FunctionNode<'a> {
        use crate::nodes::expression::ExpressionNode;
        use crate::nodes::function::{FunctionNode, ParameterNode};
        use crate::nodes::statement::StatementNode;
        use crate::nodes::types::CONSTRUCTOR_NAME;

        let params: Vec<ParameterNode> = fields
            .iter()
            .map(|f| ParameterNode::new(f.name.clone(), f.field_type.clone()))
            .collect();
        let this_tok = crate::token::syntax_token::SyntaxToken::new(
            TokenKind::IdentifierToken,
            struct_name.position.clone(),
            "this".to_string(),
        );
        let mut stmts = Vec::new();
        for f in fields {
            let this_expr = self
                .arena
                .alloc(ExpressionNode::Identifier(this_tok.clone()));
            let value = ExpressionNode::Identifier(f.name.clone());
            stmts.push(StatementNode::MemberAssignment(
                this_expr,
                f.name.clone(),
                value,
            ));
        }
        let body = self.arena.alloc_slice_clone(&stmts);
        let name = crate::token::syntax_token::SyntaxToken::new(
            TokenKind::IdentifierToken,
            struct_name.position.clone(),
            CONSTRUCTOR_NAME.to_string(),
        );
        FunctionNode::new(
            Vec::new(),
            name,
            None,
            None,
            params,
            body,
            Visibility::Public,
        )
    }
}
