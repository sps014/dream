use super::*;
use crate::errors::SemanticError;
use crate::function_control_flow::FunctionControlGraph;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{FunctionNode, StatementNode, Type};
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

impl<'a> Analyzer<'a> {
    pub(super) fn analyze_function(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Rc<RefCell<SymbolTable>>, SemanticError> {
        let param_table = Rc::new(RefCell::new(
            self.add_function_param_table(function, diagnostics)?,
        ));
        self.current_file = function.file_path.clone();
        let errors_before = diagnostics.errors().count();
        self.hir_begin_function(function);
        let is_unsafe = function.attributes.iter().any(|a| a.name.text == "unsafe");
        let is_compute = dream_abi::attributes::has_compute_attr(&function.attributes);
        let is_gpu = dream_abi::attributes::is_gpu_shader_attr(&function.attributes)
            || dream_abi::attributes::has_gpu_helper_attr(&function.attributes);
        let runtime_support =
            dream_abi::attributes::RuntimeSupport::from_attributes(&function.attributes);
        self.with_runtime_flag(runtime_support, |s| {
            s.with_unsafe_flag(is_unsafe, |s| {
                s.with_gpu_flags(is_compute, is_gpu, |s| {
                    s.with_async_flag(function.is_async, |s| {
                        s.analyze_body(
                            function.body,
                            function,
                            Some(&param_table),
                            false,
                            diagnostics,
                        )?;
                        // Enforce the v1 `await` placement rules (only in async functions, only at statement
                        // position) and that non-async functions contain no `await` at all.
                        s.check_await_positions(function, diagnostics);
                        Ok(())
                    })
                })
            })
        })?;
        self.hir_finish_function(diagnostics, errors_before);
        // Unused `let`/`const` bindings (warnings only — do not fail the compile).
        param_table
            .as_ref()
            .borrow()
            .report_unused_locals(diagnostics);
        // check return
        let mut graph = FunctionControlGraph::new(function);
        if let Err(e) = graph.build() {
            diagnostics.report_error(e.to_string(), Some(function.name.position));
        }
        Ok(param_table.clone())
    }

    pub(super) fn add_function_param_table(
        &mut self,
        function: &FunctionNode<'a>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<SymbolTable, SemanticError> {
        // Parent the parameter table off the module-global scope so function bodies resolve
        // top-level variables (and their `const`-ness) through ordinary lexical lookup.
        let mut param_table = SymbolTable::new(Some(self.global_symbol_table.clone()));
        for param in function.parameters.iter() {
            self.check_reserved_name(&param.name, "parameter", diagnostics);
            if let Err(e) = param_table.add_symbol(param.name.text.clone(), param.type_.clone()) {
                diagnostics.report_error(e.to_string(), Some(param.name.position));
            }
        }
        // A capturing lambda's synthesized function (see `expressions::lambda`) additionally binds
        // its captured name(s) — not among `function.parameters`, but ordinary type-checkable
        // locals as far as the body is concerned (its runtime storage, an unboxed `.value` read
        // through a `CaptureCell<T>` populated from `$__closure_env`, is purely a `hir_begin_function`
        // concern; see the capturing-lambda prologue there).
        if let Some(captures) = self.closure_captures.get(&function.name.text) {
            for (cap_name, cap_ty) in captures.clone() {
                let _ = param_table.add_symbol(cap_name, cap_ty);
            }
        }
        // Compute kernels get WGSL builtins as ordinary locals (`global_id.x`, …). `GpuId3` is
        // defined in `system.gpu` (auto-loaded whenever any GPU shader attr is present).
        if dream_abi::attributes::has_compute_attr(&function.attributes) {
            let id3 = Type::Struct(
                super::synthetic_token(TokenKind::IdentifierToken, "GpuId3"),
                None,
            );
            for name in ["global_id", "local_id", "workgroup_id", "num_workgroups"] {
                let _ = param_table.add_symbol(name.to_string(), id3.clone());
            }
        }
        if dream_abi::attributes::has_vertex_attr(&function.attributes) {
            let i32ty = Type::Integer(super::synthetic_token(TokenKind::DataTypeToken, "int"));
            let _ = param_table.add_symbol("vertex_index".to_string(), i32ty.clone());
            let _ = param_table.add_symbol("instance_index".to_string(), i32ty);
        }
        if dream_abi::attributes::has_fragment_attr(&function.attributes) {
            let v4 = Type::Struct(
                super::synthetic_token(TokenKind::IdentifierToken, "GpuVec4"),
                None,
            );
            let i32ty = Type::Integer(super::synthetic_token(TokenKind::DataTypeToken, "int"));
            let boolty = Type::Boolean(super::synthetic_token(TokenKind::DataTypeToken, "bool"));
            let _ = param_table.add_symbol("frag_coord".to_string(), v4);
            let _ = param_table.add_symbol("front_facing".to_string(), boolty);
            let _ = param_table.add_symbol("sample_index".to_string(), i32ty.clone());
            let _ = param_table.add_symbol("primitive_index".to_string(), i32ty);
        }
        Ok(param_table)
    }

    pub(super) fn analyze_body(
        &mut self,
        body: &[StatementNode<'a>],
        parent_function: &FunctionNode<'a>,
        parent_table: Option<&Rc<RefCell<SymbolTable>>>,
        has_parent_loop: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let parent_scope = match parent_table {
            Some(t) => Some(Rc::clone(t)),
            None => None,
        };
        let symbol_table = Rc::new(RefCell::new(SymbolTable::new(parent_scope.clone())));
        if let Some(parent_table) = parent_scope {
            (*parent_table).borrow_mut().add_child(symbol_table.clone());
        }
        for statement in body.iter() {
            let clone = &symbol_table.clone();
            // Recover at the statement boundary: a short-circuited statement leaves its diagnostic
            // in the bag, and we move on to the next sibling so every independent error in the
            // block is still reported (matching the previous poison-and-continue behavior).
            let _ = self.analyze_statement(
                statement,
                parent_function,
                clone,
                has_parent_loop,
                diagnostics,
            );
        }
        Ok(())
    }
    pub(super) fn analyze_statement(
        &mut self,
        statement: &StatementNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        has_parent_while: bool,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        let ctx = super::AnalyzerContext {
            parent_function,
            symbol_table,
        };
        // Debug-info: record the source line of this statement before lowering it, so the backend can
        // emit a host line-hook at each statement boundary. A no-op unless debug-info is enabled.
        if let Some(line) = super::statement_line(statement) {
            self.hir_mark_line(line as u32);
        }
        match statement {
            StatementNode::Declaration(left, type_annotation, right, is_const) => self
                .analyze_declaration(left, type_annotation, right, *is_const, &ctx, diagnostics)?,
            StatementNode::WorkgroupDecl(name, ty, _size) => {
                if !self.current_function_is_compute {
                    diagnostics.report_error(
                        "'@workgroup' declarations are only allowed inside '@compute' kernels"
                            .to_string(),
                        Some(name.position),
                    );
                } else {
                    let arr_ty = Type::Array(Box::new(ty.clone()));
                    if let Err(e) = symbol_table
                        .borrow_mut()
                        .add_symbol(name.text.clone(), arr_ty)
                    {
                        diagnostics.report_error(e.to_string(), Some(name.position));
                    }
                }
            }
            StatementNode::TupleDeclaration {
                pattern,
                ty,
                init,
                is_const,
            } => self.analyze_tuple_declaration(pattern, ty, init, *is_const, &ctx, diagnostics)?,
            StatementNode::Assignment(left, right) => {
                self.analyze_assignment(left, right, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::IndexAssignment(left, index, right) => self.analyze_index_assignment(
                left,
                index,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?,
            StatementNode::MemberAssignment(obj, member, right) => self.analyze_member_assignment(
                obj,
                member,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?,
            StatementNode::IfElse(..) => {
                self.analyze_if_else(statement, &ctx, has_parent_while, diagnostics)?
            }
            StatementNode::Return(expression) => {
                self.analyze_return(expression, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::While(condition, body) => {
                self.analyze_while(condition, body, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::DoWhile(body, condition) => {
                self.analyze_do_while(condition, body, parent_function, symbol_table, diagnostics)?
            }
            StatementNode::Lock(target, body) => self.analyze_lock(
                target,
                body,
                parent_function,
                symbol_table,
                has_parent_while,
                diagnostics,
            )?,
            StatementNode::For(init, condition, increment, body) => {
                self.analyze_for(init, condition, increment, body, &ctx, diagnostics)?
            }
            StatementNode::ForEach(..) => self.analyze_foreach(statement, &ctx, diagnostics)?,
            StatementNode::Switch(subject, cases, default_body) => self.analyze_case_switch(
                subject,
                cases,
                default_body,
                &ctx,
                has_parent_while,
                diagnostics,
            )?,
            StatementNode::Labeled(label, inner) => {
                self.loop_labels.push(label.clone());
                // Hand the label to the wrapped loop's analyzer so it lands on the loop's HIR node.
                self.pending_loop_label = Some(label.clone());
                let result = self.analyze_statement(
                    inner,
                    parent_function,
                    symbol_table,
                    has_parent_while,
                    diagnostics,
                );
                self.pending_loop_label = None;
                self.loop_labels.pop();
                result?;
            }
            StatementNode::Break(label) => {
                self.analyze_break(label, parent_function, has_parent_while, diagnostics)?
            }
            StatementNode::Continue(label) => {
                self.analyze_continue(label, parent_function, has_parent_while, diagnostics)?
            }
            StatementNode::FunctionInvocation(name, generic_args, params) => {
                let _ = self.analyze_function_call(
                    name,
                    generic_args,
                    params,
                    parent_function,
                    symbol_table,
                    diagnostics,
                );
                let value = self.hir_take();
                self.hir_expr_stmt(value);
            }
            StatementNode::ExpressionStatement(expr) => {
                // A statement-position pattern `switch` allows block arms and yields no value.
                if let dream_syntax::nodes::ExpressionNode::Switch(_, subject, arms) = expr {
                    // `analyze_pattern_switch` emits the `Switch` itself (or fails the function) in
                    // statement position; no separate expression-statement is needed.
                    let _ = self.analyze_pattern_switch(
                        subject,
                        arms,
                        parent_function,
                        symbol_table,
                        false,
                        diagnostics,
                    );
                } else {
                    let _ =
                        self.analyze_expression(expr, parent_function, symbol_table, diagnostics);
                    let value = self.hir_take();
                    self.hir_expr_stmt(value);
                }
            }
            StatementNode::MethodInvocation(obj, method, generic_args, params) => {
                let _ =
                    self.analyze_method_call(obj, method, generic_args, params, &ctx, diagnostics);
                let value = self.hir_take();
                self.hir_expr_stmt(value);
            }
            StatementNode::AwaitStmt(future_expr) => {
                let fut = self
                    .analyze_expression(future_expr, parent_function, symbol_table, diagnostics)
                    .unwrap_or(Type::Unknown);
                let value = self.hir_take();
                // `await <jsExpr>;` (discarding the `Option<js>` result): desugar the same way.
                if self.is_js_type(&fut) {
                    let fut_hir = self.desugar_js_await(value);
                    self.hir_await_stmt(fut_hir);
                } else if Self::future_inner_type(&fut).is_none() {
                    diagnostics.report_error(
                        format!("'await' expects a Future value, got {}", self.ty_display(&fut)),
                        future_expr.position(),
                    );
                    self.hir_fail();
                } else {
                    self.hir_await_stmt(value);
                }
            }
        };
        Ok(())
    }
}
