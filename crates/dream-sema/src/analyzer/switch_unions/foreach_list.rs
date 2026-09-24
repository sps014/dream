//! `for (let <element> in <list>)` over a statically known stdlib `List<T>`, lowered to an index
//! loop instead of the `ListIterator` + `Option<T>` enumerator protocol.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::types::mangle_generic;
use dream_syntax::nodes::{StatementNode, Type};
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use dream_types::method_fn;
use std::cell::RefCell;
use std::rc::Rc;

/// Mangled `List<T>` accessors the index loop calls.
pub(super) struct ListAccessors {
    list: Type,
    element: Type,
    length: String,
    at: String,
}

impl<'a> Analyzer<'a> {
    /// The accessors for `ty` when it is the stdlib `List<T>` (not a user type of the same name).
    pub(super) fn stdlib_list_accessors(
        &mut self,
        ty: &Type,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<ListAccessors> {
        let (mut base, mut args) = Self::resolve_struct_parts(ty)?;
        if args.is_empty() {
            let (b, a) = self
                .generic_struct_instances
                .iter()
                .find(|(b, a)| b == "List" && mangle_generic(b, a) == base)?;
            (base, args) = (b.clone(), a.clone());
        }
        if base != "List" || args.len() != 1 {
            return None;
        }
        let from_std = self
            .generic_structs
            .get("List")
            .and_then(|t| t.file_path.as_deref())
            .is_some_and(|f| f.starts_with("<std>/"));
        if !from_std {
            return None;
        }
        self.ensure_type_instantiated(&base, &args, &empty_span(), diagnostics);
        let mono = mangle_generic(&base, &args);
        let length = method_fn(&mono, &getter_member_name("length"));
        let at = method_fn(&mono, "at_unchecked");
        let resolves = |name: &str| {
            self.function_table.get_function(name).is_ok()
                && self
                    .type_ctx
                    .defs
                    .lookup(dream_types::DefKind::Function, name)
                    .is_some()
        };
        if !resolves(&length) || !resolves(&at) {
            return None;
        }
        Some(ListAccessors {
            list: ty.clone(),
            element: args[0].clone(),
            length,
            at,
        })
    }

    /// Lowers `for (let x in list)` to
    ///
    /// ```text
    /// let $list = <iterable>;
    /// let $i = 0;
    /// while (true) {
    ///     if (!($i < $list.length)) { break; }
    ///     x = $list.at_unchecked($i);
    ///     $i = $i + 1;
    ///     <body>
    /// }
    /// ```
    ///
    /// which is exactly `ListIterator.next()` inlined: `length` is re-read every step, so a list
    /// mutated during iteration behaves identically, and the increment precedes the body so
    /// `continue` still advances.
    pub(super) fn analyze_foreach_list(
        &mut self,
        element: &SyntaxToken,
        iter_hir: Option<dream_hir::HExpr>,
        acc: ListAccessors,
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
        if let Err(e) = foreach_scope
            .borrow_mut()
            .add_symbol(element.text.clone(), acc.element.clone())
        {
            diagnostics.report_error(e.to_string(), Some(element.position));
        }

        let int_type = Type::Integer(synthetic_token(TokenKind::DataTypeToken, "int"));
        let list_local = self.hir_alloc_local("$foreach_list", &acc.list);
        let idx_local = self.hir_alloc_local("$foreach_idx", &int_type);
        let elem_slot = self.hir_alloc_local(&element.text, &acc.element);

        if let (Some(list_l), Some(idx_l)) = (list_local, idx_local) {
            self.hir_assign_local_id(list_l, iter_hir);
            let zero = self.hx_int(0);
            self.hir_assign_local_id(idx_l, Some(zero));
        }

        self.hir_open_block();
        if let (Some(list_l), Some(idx_l), Some(elem_l)) = (list_local, idx_local, elem_slot) {
            let list_ty = self.type_ctx.lower(&acc.list);
            let int = self.type_ctx.interner.int();

            self.hir_set_method_call(
                Some(self.hx_local(list_l, list_ty)),
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
                Some(self.hx_local(list_l, list_ty)),
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
