use super::super::Parser;
use crate::nodes::Visibility;
use crate::token::token_kind::TokenKind;
use std::io::Error;

impl<'a, 'b> Parser<'a, 'b> {
    /// Parses a top-level variable declaration: an optional `public`/`internal` modifier,
    /// then `let`/`const`, a name, an optional `: type` annotation, a required initializer, and a
    /// terminating `;`. Returns the assembled [`GlobalVariableNode`].
    pub(crate) fn parse_global_variable(
        &mut self,
    ) -> Result<crate::nodes::GlobalVariableNode<'a>, Error> {
        let first_trivia = self.current_token().leading_trivia.clone();

        let mut visibility = Visibility::Private;
        loop {
            if self.try_consume_visibility(&mut visibility) {
                continue;
            }
            match self.current_token().kind {
                // `static` is a class-member modifier. A top-level `let` is already a single value
                // initialized once at module load, and file-private is the default visibility.
                TokenKind::StaticToken => {
                    self.diagnostics.report_error(
                        "'static' cannot modify a top-level variable; it declares class members. Top-level variables are file-private by default — use 'internal' or 'public' to widen".to_string(),
                        Some(self.current_token().position),
                    );
                    self.match_token(TokenKind::StaticToken);
                }
                _ => break,
            }
        }

        let is_const = self.current_token().kind == TokenKind::ConstToken;
        if is_const {
            self.match_token(TokenKind::ConstToken);
        } else {
            self.match_token(TokenKind::LetToken);
        }

        let mut name = self.match_token(TokenKind::IdentifierToken);
        Self::splice_leading_trivia(&mut name, first_trivia);

        let declared_type = if self.current_token().kind == TokenKind::ColonToken {
            self.match_token(TokenKind::ColonToken);
            Some(self.parse_type()?)
        } else {
            None
        };

        self.match_token(TokenKind::EqualToken);
        let initializer = self.parse_expression(0)?;
        self.match_token(TokenKind::SemicolonToken);

        Ok(crate::nodes::GlobalVariableNode {
            name,
            declared_type,
            initializer,
            is_const,
            visibility,
            file_path: None,
        })
    }
}
