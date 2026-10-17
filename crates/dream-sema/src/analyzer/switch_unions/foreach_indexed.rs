//! Trusted stdlib slot walks avoid allocating an iterator and an Option for every element.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{StatementNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

/// Mangled accessors the index loop calls.
pub(super) struct IndexedAccessors {
    seq: Type,
    element: Type,
    length: crate::function_table::FunctionIdentity,
    at: crate::function_table::FunctionIdentity,
    live: Option<crate::function_table::FunctionIdentity>,
}

impl<'a> Analyzer<'a> {
    /// Resolve the internal slot-walk contract on a trusted stdlib declaration.
    pub(super) fn stdlib_indexed_accessors(
        &mut self,
        ty: &Type,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<IndexedAccessors> {
        let id = self.type_ctx.lower(ty);
        let dream_types::TyKind::Struct(def, _) = self.type_ctx.interner.kind(id).clone() else {
            return None;
        };
        let template = *self.generic_struct(def)?;
        if !template
            .file_path
            .as_deref()
            .is_some_and(dream_stdlib::is_std_source)
        {
            return None;
        }
        let attrs = &template.attributes;
        let count_name = dream_abi::attributes::named_attr_string_arg(attrs, "slot_walk", 0)?;
        let value_name = dream_abi::attributes::named_attr_string_arg(attrs, "slot_walk", 1)?;
        let live_name = dream_abi::attributes::named_attr_string_arg(attrs, "slot_walk", 2)
            .filter(|name| !name.is_empty());
        let base = self.type_ctx.syntax_type(id);
        let (name, arguments) = Self::resolve_struct_parts(&base)?;
        self.ensure_type_instantiated(&name, &arguments, &empty_span(), diagnostics);
        let length_info = self.method_info(id, &getter_member_name(count_name)).ok()?;
        let length = length_info.identity;
        let at_info = self.method_info(id, value_name).ok()?;
        let element = at_info.return_type.clone()?;
        let at = at_info.identity;
        let live = if let Some(name) = live_name {
            Some(self.method_info(id, name).ok()?.identity)
        } else {
            None
        };
        Some(IndexedAccessors {
            seq: ty.clone(),
            element,
            length,
            at,
            live,
        })
    }

    /// Lowers `for (let x in seq)` to
    ///
    /// ```text
    /// let $seq = <iterable>;
    /// let $i = 0;
    /// while (true) {
    ///     if (!($i < $seq.length)) { break; }
    ///     x = $seq.at_unchecked($i);
    ///     $i = $i + 1;
    ///     <body>
    /// }
    /// ```
    ///
    /// which is exactly `ListIterator.next()` inlined: `length` is re-read every step, so a list
    /// mutated during iteration behaves identically, and the increment precedes the body so
    /// `continue` still advances.
    pub(super) fn analyze_foreach_indexed(
        &mut self,
        element: &SyntaxToken,
        iter_hir: Option<dream_hir::HExpr>,
        acc: IndexedAccessors,
        body: &[StatementNode<'a>],
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), SemanticError> {
        use dream_hir::{BinOp, HExpr, HExprKind, HStmt, Overflow};

        let label = self.pending_loop_label.take();
        let foreach_scope = Rc::new(RefCell::new(SymbolTable::new(Some(
            ctx.symbol_table.clone(),
        ))));
        (*ctx.symbol_table)
            .borrow_mut()
            .add_child(foreach_scope.clone());
        {
            let mut scope = foreach_scope.borrow_mut();
            // Visible to the `await` scope rule: a span iterated here lives across the body.
            let _ = scope.add_symbol("__foreach_seq".to_string(), acc.seq.clone());
            if let Err(e) = scope.add_symbol(element.text.clone(), acc.element.clone()) {
                diagnostics.report_error(e.to_string(), Some(element.position));
            }
        }

        let int_type = Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int"));
        let seq_local = self.hir_alloc_local("$foreach_seq", &acc.seq);
        let idx_local = self.hir_alloc_local("$foreach_idx", &int_type);
        let elem_slot = self.hir_alloc_local(&element.text, &acc.element);

        if let (Some(seq_l), Some(idx_l)) = (seq_local, idx_local) {
            self.hir_assign_local_id(seq_l, iter_hir);
            let zero = self.hx_int(0);
            self.hir_assign_local_id(idx_l, Some(zero));
        }

        self.hir_open_block();
        if let (Some(seq_l), Some(idx_l), Some(elem_l)) = (seq_local, idx_local, elem_slot) {
            let seq_ty = self.type_ctx.lower(&acc.seq);
            let int = self.type_ctx.interner.int();

            self.hir_set_method_call(
                Some(self.hx_local(seq_l, seq_ty)),
                &acc.length,
                vec![],
                &int_type,
            );
            let len = self.hir_take();
            let in_range = len.map(|len| self.hx_bin(BinOp::Lt, self.hx_local(idx_l, int), len));
            if let Some(in_range) = in_range {
                let exhausted = self.hx_not(in_range);
                self.hir_push_stmt(HStmt::If {
                    cond: exhausted,
                    then_branch: vec![HStmt::Break(None)],
                    else_branch: vec![],
                });
            } else {
                self.hir_fail();
            }

            if let Some(live) = &acc.live {
                let bool_type = self.type_ctx.syntax_type(self.type_ctx.interner.bool());
                self.hir_set_method_call(
                    Some(self.hx_local(seq_l, seq_ty)),
                    live,
                    vec![Some(self.hx_local(idx_l, int))],
                    &bool_type,
                );
                if let Some(occupied) = self.hir_take() {
                    self.hir_push_stmt(HStmt::If {
                        cond: self.hx_not(occupied),
                        then_branch: vec![
                            HStmt::Assign {
                                place: dream_hir::HPlace::Local(idx_l),
                                value: self.hx_bin(
                                    BinOp::Add,
                                    self.hx_local(idx_l, int),
                                    self.hx_int(1),
                                ),
                            },
                            HStmt::Continue(None),
                        ],
                        else_branch: vec![],
                    });
                } else {
                    self.hir_fail();
                }
            }

            self.hir_set_method_call(
                Some(self.hx_local(seq_l, seq_ty)),
                &acc.at,
                vec![Some(self.hx_local(idx_l, int))],
                &acc.element,
            );
            let at = self.hir_take();
            self.hir_assign_local_id(elem_l, at);

            let next = HExpr::new(
                int,
                HExprKind::Binary {
                    op: BinOp::Add,
                    lhs: Box::new(self.hx_local(idx_l, int)),
                    rhs: Box::new(self.hx_int(1)),
                    overflow: Overflow::Wrapping,
                },
            );
            self.hir_assign_local_id(idx_l, Some(next));
        }
        self.hir_box_captured_binding(&element.text, &acc.element);

        self.analyze_body(
            body,
            ctx.parent_function,
            Some(&foreach_scope),
            true,
            diagnostics,
        )?;
        let body_hir = self.hir_close_block();
        let true_lit = self.hx_bool(true);
        self.hir_while(Some(true_lit), body_hir, label);
        Ok(())
    }
}
