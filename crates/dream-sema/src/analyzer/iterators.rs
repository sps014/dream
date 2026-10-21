use super::*;
use dream_abi::names::THIS_NAME;
use dream_syntax::nodes::{LambdaBody, LambdaNode, ParameterNode, StatementNode};

pub(super) const CHANNEL: &str = "$yield_channel";

pub(super) fn is_producer(function: &FunctionNode<'_>) -> bool {
    function.parameters.iter().any(|p| p.name.text == CHANNEL)
}

impl<'a> Analyzer<'a> {
    pub(super) fn iterator_factory(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<&'a FunctionNode<'a>> {
        let ret = function.return_type.as_ref().map(|ty| {
            self.type_ctx
                .lower(&Self::monomorphize_type(ty, &self.current_generic_bindings))
        })?;
        let element_id = dream_abi::iterators::element(&self.type_ctx, ret, function.is_async)?;
        let element = self.type_ctx.syntax_type(element_id);
        if function.is_extern || is_producer(function) {
            return None;
        }
        if !function.body.iter().any(StatementNode::contains_yield) {
            if function.is_async {
                diagnostics.report_error(
                    "an async iterator body must contain 'yield' or 'yield break'".into(),
                    Some(function.name.position),
                );
            }
            return None;
        }
        if function.accessor.is_some() {
            diagnostics.report_error(
                "iterator accessors are not supported".into(),
                Some(function.name.position),
            );
        }
        if !function.is_async {
            self.check_await_positions(function, diagnostics);
        }
        let source = LambdaNode {
            open_paren_position: function.name.position,
            async_keyword: None,
            is_async: function.is_async,
            generic_parameters: None,
            generic_constraints: Vec::new(),
            parameters: Vec::new(),
            body: LambdaBody::Block(function.body),
        };
        let used = super::expressions::capture_scan::lambda_free_names(&source);
        let mut factory = function.clone();
        factory.is_async = false;
        let mut body = Vec::new();
        let mut factory_body = Vec::new();
        for param in &mut factory.parameters {
            if param.is_ref || param.is_borrow {
                diagnostics.report_error(
                    "iterator parameters cannot be ref or borrow".into(),
                    Some(param.name.position),
                );
            }
            if !used.contains(&param.name.text) {
                continue;
            }
            let original = param.name.clone();
            let mut captured = original.clone();
            captured.text = format!("$yield_capture_{}", original.text);
            if original.text == THIS_NAME {
                factory_body.push(StatementNode::Declaration(
                    captured.clone(),
                    Some(param.type_.clone()),
                    ExpressionNode::Identifier(original.clone()),
                    true,
                    false,
                ));
            } else {
                param.name = captured.clone();
            }
            body.push(StatementNode::Declaration(
                original,
                Some(param.type_.clone()),
                ExpressionNode::Identifier(captured),
                false,
                false,
            ));
        }
        // Parameter seeds belong outside the source body's scope, just like real parameters.
        body.push(StatementNode::Overflow(
            dream_syntax::nodes::OverflowMode::Unchecked,
            synthetic_token(TokenKind::IdentifierToken, "unchecked"),
            function.body,
        ));
        body.push(StatementNode::ExpressionStatement(
            self.channel_call("finish", Vec::new()),
        ));
        let lambda = self.arena.alloc(LambdaNode {
            open_paren_position: function.name.position,
            async_keyword: None,
            is_async: true,
            generic_parameters: None,
            generic_constraints: Vec::new(),
            parameters: vec![ParameterNode::new(
                synthetic_token(TokenKind::IdentifierToken, CHANNEL),
                Type::Struct(
                    synthetic_token(TokenKind::IdentifierToken, dream_abi::iterators::CHANNEL),
                    Some(vec![element.clone()]),
                ),
            )],
            body: LambdaBody::Block(self.arena.alloc_slice_fill_iter(body)),
        });
        let ctor = if function.is_async {
            dream_abi::iterators::ASYNC_SEQUENCE
        } else {
            dream_abi::iterators::SEQUENCE
        };
        let call = ExpressionNode::FunctionCall(
            synthetic_token(TokenKind::IdentifierToken, ctor),
            Some(vec![element]),
            vec![ExpressionNode::Lambda(lambda)],
        );
        factory_body.push(StatementNode::Return(Some(call)));
        factory.body = self.arena.alloc_slice_fill_iter(factory_body);
        let factory = self.arena.alloc(factory);
        if let Some(identity) = self.function_declaration(function) {
            self.function_table.record_declaration(factory, identity);
        }
        Some(factory)
    }

    pub(super) fn analyze_async_foreach(
        &mut self,
        keyword: &SyntaxToken,
        inner: &StatementNode<'a>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        if !ctx.parent_function.is_async {
            diagnostics.report_error(
                "'await for' requires an async function".into(),
                Some(keyword.position),
            );
            return Ok(());
        }
        let StatementNode::ForEach(element, iterable, _, _, body) = inner else {
            return Ok(());
        };
        let ty = self
            .analyze_expression(iterable, ctx.parent_function, ctx.symbol_table, diagnostics)
            .unwrap_or(Type::Unknown);
        let expression = self.hir_take();
        let id = self.type_ctx.lower(&ty);
        let valid = match self.type_ctx.interner.kind(id) {
            dream_types::TyKind::Interface(def, _) => {
                matches!(
                    self.type_ctx.defs.name(*def),
                    "AsyncIterable" | "AsyncIterator"
                ) && self.interface_decl(*def).is_some_and(|d| {
                    d.file_path
                        .as_deref()
                        .is_some_and(dream_stdlib::is_std_source)
                })
            }
            _ => false,
        };
        if !valid {
            diagnostics.report_error(
                "'await for' requires AsyncIterable<T> or AsyncIterator<T>".into(),
                iterable.position(),
            );
            return Ok(());
        }
        self.reject_ref_struct_across_await(ctx.symbol_table, Some(keyword.position), diagnostics);
        self.analyze_foreach_iface(element, &ty, expression, body, ctx, diagnostics)
    }

    pub(super) fn iterator_cleanup(
        &mut self,
        cursor_type: &Type,
        element_type: &Type,
        asynchronous: bool,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<dream_hir::HExpr>, SemanticError> {
        ctx.symbol_table
            .borrow_mut()
            .add_symbol("$foreach_it".into(), cursor_type.clone())
            .ok();
        let class = if asynchronous {
            dream_abi::iterators::ASYNC_LEASE
        } else {
            dream_abi::iterators::LEASE
        };
        let lease = synthetic_token(TokenKind::IdentifierToken, "$foreach_lease");
        let ctor = ExpressionNode::FunctionCall(
            synthetic_token(TokenKind::IdentifierToken, class),
            Some(vec![element_type.clone()]),
            vec![ExpressionNode::Identifier(synthetic_token(
                TokenKind::IdentifierToken,
                "$foreach_it",
            ))],
        );
        self.analyze_statement(
            &StatementNode::Declaration(lease.clone(), None, ctor, false, false),
            ctx.parent_function,
            ctx.symbol_table,
            false,
            diagnostics,
        )?;
        let close = ExpressionNode::MethodCall(
            self.arena.alloc(ExpressionNode::Identifier(lease)),
            synthetic_token(TokenKind::IdentifierToken, "close"),
            None,
            Vec::new(),
        );
        self.analyze_expression(&close, ctx.parent_function, ctx.symbol_table, diagnostics)?;
        Ok(self.hir_take())
    }

    fn channel_call(&self, name: &str, args: Vec<ExpressionNode<'a>>) -> ExpressionNode<'a> {
        ExpressionNode::MethodCall(
            self.arena.alloc(ExpressionNode::Identifier(synthetic_token(
                TokenKind::IdentifierToken,
                CHANNEL,
            ))),
            synthetic_token(TokenKind::IdentifierToken, name),
            None,
            args,
        )
    }

    pub(super) fn analyze_yield(
        &mut self,
        keyword: &SyntaxToken,
        value: &Option<ExpressionNode<'a>>,
        ctx: &AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        if !is_producer(ctx.parent_function) {
            diagnostics.report_error(
                "'yield' requires an Iterable<T> or async AsyncIterable<T> function".into(),
                Some(keyword.position),
            );
            return Ok(());
        }
        if let Some(value) = value {
            let mut expression = self.channel_call("emit", vec![value.clone()]);
            if let ExpressionNode::MethodCall(_, method, _, _) = &mut expression {
                method.position = keyword.position;
            }
            self.analyze_statement(
                &StatementNode::AwaitStmt(expression),
                ctx.parent_function,
                ctx.symbol_table,
                false,
                diagnostics,
            )
        } else {
            self.analyze_statement(
                &StatementNode::ExpressionStatement(self.channel_call("finish", Vec::new())),
                ctx.parent_function,
                ctx.symbol_table,
                false,
                diagnostics,
            )?;
            self.analyze_return(&None, ctx.parent_function, ctx.symbol_table, diagnostics)
        }
    }
}
