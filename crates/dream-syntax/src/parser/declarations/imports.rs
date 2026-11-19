use super::super::Parser;
use crate::nodes::program::CImport;
use crate::nodes::{ImportNode, ModuleDeclNode};
use crate::token::syntax_token::SyntaxToken;
use crate::token::token_kind::TokenKind;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses an import statement. Two forms, told apart by a trailing `as` clause:
    /// - `import a.b.c;` — maps each dotted segment to a directory separator (`a/b/c`) so file
    ///   resolution can append the `.dream` extension later, exactly as before `as` existed.
    /// - `import a.b.c as x;` — keeps the path dot-joined (`a.b.c`): it names an item inside a
    ///   *declared* `module` namespace, not a file path, resolved separately after all files load.
    pub(crate) fn parse_import(&mut self) -> ImportNode {
        if self.current_token().kind == TokenKind::CImportToken {
            return self.parse_c_import();
        }
        if self.current_token().kind == TokenKind::JsImportToken {
            self.next_token();
            let module_name = self.match_token(TokenKind::StringToken);
            let alias = if self.current_token().kind == TokenKind::AsToken {
                self.next_token();
                Some(self.match_token(TokenKind::IdentifierToken))
            } else {
                None
            };
            self.match_token(TokenKind::SemicolonToken);
            return ImportNode {
                module_name,
                alias,
                is_js: true,
                c: None,
            };
        }
        self.match_token(TokenKind::ImportToken);

        let first = self.match_token(TokenKind::IdentifierToken);
        let mut position = first.position;
        let mut slash_path = first.text.clone();
        let mut dot_path = first.text.clone();

        while self.current_token().kind == TokenKind::DotToken {
            self.match_token(TokenKind::DotToken);
            let segment = self.match_token(TokenKind::IdentifierToken);
            position.end = segment.position.end;
            slash_path.push('/');
            slash_path.push_str(&segment.text);
            dot_path.push('.');
            dot_path.push_str(&segment.text);
        }

        if self.current_token().kind == TokenKind::AsToken {
            self.match_token(TokenKind::AsToken);
            let alias = self.match_token(TokenKind::IdentifierToken);
            self.match_token(TokenKind::SemicolonToken);
            let module_path = SyntaxToken::new(TokenKind::IdentifierToken, position, dot_path);
            return ImportNode::with_alias(module_path, alias);
        }

        self.match_token(TokenKind::SemicolonToken);

        let module_name = SyntaxToken::new(TokenKind::IdentifierToken, position, slash_path);
        ImportNode::new(module_name)
    }

    fn parse_c_import(&mut self) -> ImportNode {
        use std::hash::{DefaultHasher, Hash, Hasher};

        self.next_token();
        let module_name = self.match_token(TokenKind::StringToken);
        let library = if self.current_token().kind == TokenKind::IdentifierToken
            && self.current_token().text == "from"
        {
            self.next_token();
            Some(self.match_token(TokenKind::StringToken))
        } else {
            None
        };
        let alias = if self.current_token().kind == TokenKind::AsToken {
            self.next_token();
            Some(self.match_token(TokenKind::IdentifierToken))
        } else {
            let name = module_name
                .text
                .trim_matches('"')
                .rsplit('/')
                .next()
                .unwrap_or_default();
            let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
            let mut lexer = crate::lexer::Lexer::new(stem.to_owned());
            let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
            let tokens = lexer.lex_all(&mut diagnostics);
            if matches!(tokens.as_slice(), [token, _] if token.kind == TokenKind::IdentifierToken && token.text == stem)
            {
                Some(SyntaxToken::new(
                    TokenKind::IdentifierToken,
                    module_name.position,
                    stem.to_owned(),
                ))
            } else {
                self.diagnostics.report_error(
                    "this C header requires an explicit namespace alias".into(),
                    Some(module_name.position),
                );
                None
            }
        };
        self.match_token(TokenKind::SemicolonToken);
        let mut hash = DefaultHasher::new();
        (
            &self.c_import_scope,
            &module_name.text,
            library.as_ref().map(|token| &token.text),
        )
            .hash(&mut hash);
        ImportNode {
            module_name,
            alias,
            is_js: false,
            c: Some(CImport {
                library,
                prefix: format!("__dream_c_{:016x}_", hash.finish()),
                uses: Vec::new(),
            }),
        }
    }

    pub(crate) fn c_qualified_name(&mut self) -> Option<SyntaxToken> {
        if self.current_token().kind != TokenKind::IdentifierToken
            || self.peek_token(1).kind != TokenKind::DotToken
        {
            return None;
        }
        let alias = self.current_token().text.clone();
        let prefix = self.c_import_names.get(&alias)?.clone();
        let start = self.next_token();
        self.next_token();
        let member = self.match_token(TokenKind::IdentifierToken);
        self.c_import_uses
            .entry(alias)
            .or_default()
            .push(member.clone());
        let mut span = start.position;
        span.end = member.position.end;
        Some(SyntaxToken::new(
            TokenKind::IdentifierToken,
            span,
            format!("{prefix}{}", member.text),
        ))
    }

    /// Parses a file-scoped `module a.b.c;` declaration. Must be the first item in the file
    /// (checked by the caller, `parse_program`); at most one per file (checked by the caller too,
    /// since a second occurrence would otherwise just parse as an ordinary statement-position
    /// error). The path is kept dot-joined, mirroring the `module` namespaces it names (never a
    /// filesystem path, unlike a plain `import`).
    pub(crate) fn parse_module_decl(&mut self) -> ModuleDeclNode {
        self.match_token(TokenKind::ModuleToken);

        let first = self.match_token(TokenKind::IdentifierToken);
        let mut position = first.position;
        let mut path = first.text.clone();

        while self.current_token().kind == TokenKind::DotToken {
            self.match_token(TokenKind::DotToken);
            let segment = self.match_token(TokenKind::IdentifierToken);
            position.end = segment.position.end;
            path.push('.');
            path.push_str(&segment.text);
        }

        self.match_token(TokenKind::SemicolonToken);

        let path = SyntaxToken::new(TokenKind::IdentifierToken, position, path);
        ModuleDeclNode {
            attributes: Vec::new(),
            path,
        }
    }

    /// Parses `@attrs module a.b.c;` — attributes are collected by the caller and passed in.
    pub(crate) fn parse_module_decl_with_attrs(
        &mut self,
        attributes: Vec<crate::nodes::AttributeNode>,
    ) -> ModuleDeclNode {
        let mut decl = self.parse_module_decl();
        decl.attributes = attributes;
        decl
    }
}
