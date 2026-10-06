//! `for (let <element> in <seq>)` over a statically known stdlib `List<T>`, `Span<T>`, or
//! `ReadOnlySpan<T>`, lowered to an index loop instead of an enumerator + `Option<T>` protocol.
//! For the spans this is also what keeps iteration by-value: no enumerator object exists.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{StatementNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

/// Stdlib generic sequences whose for-each is an index loop over `length` + `at_unchecked`.
const INDEXED_SEQUENCES: &[&str] = &["List", "Span", "ReadOnlySpan"];

/// Mangled accessors the index loop calls.
pub(super) struct IndexedAccessors {
    seq: Type,
    element: Type,
    length: crate::function_table::FunctionIdentity,
    at: crate::function_table::FunctionIdentity,
}

impl<'a> Analyzer<'a> {
    /// The accessors for `ty` when it is one of the stdlib [`INDEXED_SEQUENCES`] (not a user type
    /// of the same name).
    pub(super) fn stdlib_indexed_accessors(
        &mut self,
        ty: &Type,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<IndexedAccessors> {
        let id = self.type_ctx.lower(ty);
        let dream_types::TyKind::Struct(def, args) = self.type_ctx.interner.kind(id).clone() else {
            return None;
        };
        let template = *self.generic_struct(def)?;
        if !INDEXED_SEQUENCES.contains(&template.name.text.as_str())
            || args.len() != 1
            || !template
                .file_path
                .as_deref()
                .is_some_and(dream_stdlib::is_std_source)
        {
            return None;
        }
        let base = self.type_ctx.syntax_type(id);
        let (name, arguments) = Self::resolve_struct_parts(&base)?;
        self.ensure_type_instantiated(&name, &arguments, &empty_span(), diagnostics);
        let length = self
            .method_info(id, &getter_member_name("length"))
            .ok()?
            .identity;
        let at = self.method_info(id, "at_unchecked").ok()?.identity;
        let args: Vec<_> = args
            .iter()
            .map(|&arg| self.type_ctx.syntax_type(arg))
            .collect();
        Some(IndexedAccessors {
            seq: ty.clone(),
            element: args[0].clone(),
            length,
            at,
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
