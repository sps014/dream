//! The `analyze_expression` dispatch match and the class-indexer read desugar it delegates to.

use super::*;
use crate::errors::SemanticError;
use crate::symbol_table::SymbolTable;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, LambdaBody, LambdaNode, Type};
use dream_syntax::token::token_kind::TokenKind;
use std::cell::RefCell;
use std::rc::Rc;

mod collections;
mod literals;
mod operators;

impl<'a> Analyzer<'a> {
    pub(in crate::analyzer) fn analyze_expression(
        &mut self,
        expression: &ExpressionNode<'a>,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        match expression {
            ExpressionNode::Literal(..) => self.analyze_literals_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::ArrayLiteral(..) => self.analyze_collections_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::ArrayRepeat(..) => self.analyze_collections_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::TupleLiteral(..) => self.analyze_literals_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::SetLiteral(..) => self.analyze_collections_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::MapLiteral(..) => self.analyze_collections_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::IndexAccess(..) => self.analyze_operators_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::Unary(..) => self.analyze_operators_expression(
                expression,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::IncDec {
                prefix,
                is_inc,
                target,
                op,
            } => self.analyze_inc_dec(
                (*prefix, *is_inc),
                target,
                op,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::Binary(left, opr, right) => Ok(self.analyze_binary_expression(
                left,
                opr,
                right,
                parent_function,
                symbol_table,
                diagnostics,
            )?),
            ExpressionNode::Identifier(id) => {
                // An `is`-binding introduced earlier in the same top-level `&&` chain (see
                // `analyze_binary_expression`) shadows an ordinary local of the same name, exactly
                // like the real branch-body binding does.
                let alias = self
                    .is_binding_aliases
                    .iter()
                    .rev()
                    .find(|(name, _, _)| name == &id.text)
                    .map(|(_, ty, operand)| (ty.clone(), *operand));
                if let Some((target_ty, operand)) = alias {
                    return self.analyze_cast(
                        &target_ty,
                        operand,
                        parent_function,
                        symbol_table,
                        diagnostics,
                    );
                }
                Ok(self.analyze_identifier(id, parent_function, symbol_table, diagnostics)?)
            }
            ExpressionNode::FunctionCall(name, generic_args, params) => {
                // `analyze_function_call` records the call's HIR itself (only for a resolvable,
                // non-generic, non-overloaded, non-async free function; otherwise it clears `last`).
                let t = self.analyze_function_call(
                    name,
                    generic_args,
                    params,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::Call(callee, generic_args, params) => self.analyze_expr_call(
                callee,
                generic_args,
                params,
                parent_function,
                symbol_table,
                diagnostics,
            ),
            ExpressionNode::IsExpression(left, right_type, _binding) => {
                // `is` always evaluates to a bool. A concrete static operand folds to a compile-time
                // result; an `object` or interface-typed operand emits a runtime `$object_tag`
                // comparison. (The optional `_binding` is handled by the statement layer — `if`/
                // `while` conditions and top-level `&&` chains, see `statements.rs` — which flow-types
                // it into the guarded branch/body; the expression itself ignores the binding here.)
                let left_type =
                    self.analyze_expression(left, parent_function, symbol_table, diagnostics)?;
                self.check_type_not_static_class(right_type, diagnostics);
                let left_hir = self.hir_take();
                let left_name = left_type.get_type();
                if left_type.is_unknown() {
                    self.hir_none();
                } else if left_name == "object" || self.is_interface_name(&left_name) {
                    self.hir_set_is_type(left_hir, right_type);
                } else {
                    let left_id = self.type_ctx.lower(&left_type);
                    let right_id = self.type_ctx.lower(right_type);
                    self.hir_set_bool(left_id == right_id);
                }
                Ok(Type::Boolean(synthetic_token(
                    TokenKind::BooleanToken,
                    "true",
                )))
            }
            ExpressionNode::Parenthesized(_, expr) => {
                Ok(self.analyze_expression(expr, parent_function, symbol_table, diagnostics)?)
            }
            ExpressionNode::Try(inner) => Ok(self.analyze_try_expression(
                inner,
                parent_function,
                symbol_table,
                diagnostics,
            )?),
            ExpressionNode::Lambda(lambda) => {
                Ok(self.analyze_lambda(lambda, parent_function, symbol_table, diagnostics)?)
            }
            ExpressionNode::Ternary(condition, then_expr, else_expr) => {
                let cond_type =
                    self.analyze_expression(condition, parent_function, symbol_table, diagnostics)?;
                let cond_hir = self.hir_take();
                if !cond_type.is_bool() {
                    diagnostics.report_error(
                        format!(
                            "Ternary condition must be of type bool, got {}",
                            self.ty_display(&cond_type)
                        ),
                        condition.position(),
                    );
                }
                let then_type =
                    self.analyze_expression(then_expr, parent_function, symbol_table, diagnostics)?;
                let then_hir = self.hir_take();
                let else_type =
                    self.analyze_expression(else_expr, parent_function, symbol_table, diagnostics)?;
                let else_hir = self.hir_take();
                // Both branches must agree; reuse the standard compatibility check.
                self.compare_data_type(
                    &then_type,
                    &else_type,
                    &else_expr.position().unwrap_or_else(empty_span),
                    diagnostics,
                )?;
                self.hir_set_ternary(cond_hir, then_hir, else_hir, &then_type);
                Ok(then_type)
            }
            ExpressionNode::Switch(_, subject, arms) => {
                // `analyze_pattern_switch` desugars the value-position switch and records its result temp read.
                let t = self.analyze_pattern_switch(
                    subject,
                    arms,
                    parent_function,
                    symbol_table,
                    true,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::MemberAccess(obj, member) => {
                // `analyze_member_access` records the HIR itself (struct-field read / enum value).
                let t = self.analyze_member_access(
                    obj,
                    member,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::Cast(_, target_type, expr) => {
                // `analyze_cast` records the cast's HIR itself.
                let t = self.analyze_cast(
                    target_type,
                    expr,
                    parent_function,
                    symbol_table,
                    diagnostics,
                )?;
                Ok(t)
            }
            ExpressionNode::SizeOf(_, ty) => self.analyze_sizeof(ty, diagnostics),
            ExpressionNode::NameOf(_, parts) => self.analyze_nameof(parts, diagnostics),
            ExpressionNode::TypeOf(_, operand) => {
                self.analyze_typeof(operand, parent_function, symbol_table, diagnostics)
            }
            ExpressionNode::MethodCall(obj, method, generic_args, params) => {
                let ctx = super::super::AnalyzerContext {
                    parent_function,
                    symbol_table,
                };
                let t =
                    self.analyze_method_call(obj, method, generic_args, params, &ctx, diagnostics)?;
                // `analyze_method_call` records the `MethodCall`/`Call` (or clears `last`) itself.
                Ok(t)
            }
            ExpressionNode::Await(_, inner) => {
                let fut =
                    self.analyze_expression(inner, parent_function, symbol_table, diagnostics)?;
                let inner_hir = self.hir_take();
                if fut.is_unknown() {
                    self.hir_none();
                    return Ok(Type::Unknown);
                }
                // Awaiting a dynamic `js` value treats it as a JS Promise: desugar to
                // `js.await_promise(inner).await`, whose async bridge yields `Future<js>` and resolves to
                // the awaited value as another `js`.
                if self.is_js_type(&fut) {
                    let fut_hir = self.desugar_js_await(inner_hir);
                    let opt = Self::option_js_type();
                    self.hir_set_await(fut_hir, &opt);
                    return Ok(opt);
                }
                match Self::future_inner_type(&fut) {
                    Some(t) => {
                        self.hir_set_await(inner_hir, &t);
                        Ok(t)
                    }
                    None => {
                        self.hir_none();
                        Err(report(
                            diagnostics,
                            format!(
                                "'await' expects a Future value, got {}",
                                self.ty_display(&fut)
                            ),
                            inner.position(),
                        ))
                    }
                }
            }
            // A named argument (`name: value`) is only meaningful inside a call's argument list,
            // where `normalize_named_arguments` resolves and strips it before any argument reaches
            // general expression analysis. Reaching this arm means one appeared somewhere else
            // (e.g. `[a: 1]`) — report it as a diagnostic rather than silently analyzing `value`.
            ExpressionNode::NamedArg(name, _) => {
                self.hir_none();
                Err(report(
                    diagnostics,
                    format!("named argument '{}' is not allowed here", name.text),
                    Some(name.position),
                ))
            }
            // A `ref` argument (`f(ref x)`) is only meaningful inside a call's argument list,
            // where the call-analysis paths (`analyze_ref_argument`) resolve and strip it before
            // any argument reaches general expression analysis. Reaching this arm means one
            // appeared somewhere else (e.g. `let y = ref x;`) — report it, don't silently analyze
            // the inner place as if `ref` weren't there.
            ExpressionNode::RefArgument(_, inner) => {
                self.hir_none();
                Err(report(
                    diagnostics,
                    "'ref' is only allowed as a call argument".to_string(),
                    inner.position(),
                ))
            }
            // Syntax DSL blocks must be expanded by the generate pipeline before analysis.
            ExpressionNode::SyntaxBlock(block) => {
                self.hir_none();
                Err(report(
                    diagnostics,
                    format!(
                        "unexpanded syntax block '{}'; no generator ran for this introducer",
                        block.name.text
                    ),
                    Some(block.name.position),
                ))
            }
        }
    }

    /// Desugars a class indexer read `obj[index]` to a call of the type's `@get_indexer` method when
    /// registered (see [`declarations::protocol_hooks`]): an accessible instance, non-async
    /// method taking one argument and returning a (non-`void`) value.
    fn analyze_index_get(
        &mut self,
        array_expr: &'a ExpressionNode<'a>,
        index_expr: &'a ExpressionNode<'a>,
        obj_type: &Type,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        use crate::analyzer::declarations::protocol_hooks::ProtocolRole;
        let pretty_obj = self.ty_display(obj_type);
        let (hook, info) = match self.resolve_hook_or_diagnose(
            obj_type,
            ProtocolRole::Get,
            array_expr.position(),
            true,
            diagnostics,
            || {
                format!(
                    "type '{}' has no indexer (define 'fun this[index]: T' to allow obj[index])",
                    pretty_obj
                )
            },
        ) {
            Some(resolved) => resolved,
            None => return Ok(Type::Unknown),
        };
        if matches!(info.return_type, None | Some(Type::Void)) {
            self.hir_fail();
            self.hir_none();
            diagnostics.report_error(
                format!(
                    "type '{}' has no indexer: its get indexer must return a value",
                    self.ty_display(obj_type)
                ),
                array_expr.position(),
            );
            return Ok(Type::Unknown);
        }
        let get_tok = synthetic_token(TokenKind::IdentifierToken, &hook.surface_name);
        let call =
            ExpressionNode::MethodCall(array_expr, get_tok, None, vec![(*index_expr).clone()]);
        self.analyze_expression(&call, parent_function, symbol_table, diagnostics)
    }

    /// True for scalar value types whose all-zero bit pattern is a valid value (`int`,
    /// `float`, `bool`, …). Excludes `string`/objects/classes, where zero means a null ref.
    fn is_scalar_value_type(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Integer(_)
                | Type::Float(_)
                | Type::Double(_)
                | Type::Boolean(_)
                | Type::Byte(_)
                | Type::Char(_)
                | Type::Long(_)
                | Type::UInt(_)
                | Type::ULong(_)
                | Type::ISize(_)
                | Type::USize(_)
        )
    }

    /// True for a literal whose every slot would already be produced by runtime zero-fill
    /// (`0`, `0.0`, `false`, `0` bytes), letting `[v; n]` skip the per-slot fill loop.
    fn is_zero_like_literal(expr: &ExpressionNode) -> bool {
        match expr {
            ExpressionNode::Literal(Type::Integer(t)) | ExpressionNode::Literal(Type::Byte(t)) => {
                t.text == "0"
            }
            ExpressionNode::Literal(Type::Float(t)) | ExpressionNode::Literal(Type::Double(t)) => {
                matches!(t.text.as_str(), "0" | "0.0")
            }
            ExpressionNode::Literal(Type::Boolean(t)) => t.text == "false",
            _ => false,
        }
    }

    /// True when the top level of `expr` constructs an array (`[...]` literal, `[v; n]` repeat,
    /// possibly parenthesized). Only then does `[v; n]` re-evaluate the value per slot so each
    /// row is distinct; deeper occurrences evaluate once like any other expression.
    fn is_array_construction(expr: &ExpressionNode) -> bool {
        match expr {
            ExpressionNode::Parenthesized(_, inner) => Self::is_array_construction(inner),
            ExpressionNode::ArrayLiteral(..) | ExpressionNode::ArrayRepeat(..) => true,
            _ => false,
        }
    }

    /// If `t` is `{name}<A>` (a one-generic-argument struct named `name`, e.g. `List<int>`),
    /// returns `A`. Used to recognize an expected `List<T>`/`Set<T>` target type for collection
    /// literal lowering.
    pub(in crate::analyzer) fn collection_generic_arg(t: &Type, name: &str) -> Option<Type> {
        match t {
            Type::Struct(tok, Some(args)) if tok.text == name && args.len() == 1 => {
                Some(args[0].clone())
            }
            _ => None,
        }
    }

    /// Like [`Self::collection_generic_arg`], but for a two-generic-argument struct (`Map<K, V>`).
    fn collection_generic_arg2(t: &Type, name: &str) -> Option<(Type, Type)> {
        match t {
            Type::Struct(tok, Some(args)) if tok.text == name && args.len() == 2 => {
                Some((args[0].clone(), args[1].clone()))
            }
            _ => None,
        }
    }

    /// Lowers a collection literal (`[...]` as `List<T>`, `{...}` as `Set<T>`/`Map<K, V>`) into a
    /// single synthetic static-factory call `{base}<{type_args}>.{method}(args)` and replays it
    /// through the ordinary `analyze_expression` path, reusing the existing generic-class
    /// static-dispatch machinery (the same one `Cache<int>.make(...)` uses) verbatim — no new HIR
    /// shape, no per-element codegen. `args` are typically one or two synthetic `ArrayLiteral`
    /// nodes wrapping the literal's original element/key/value sub-expressions; their element types
    /// are inferred from the (now type-argument-aware, see `analyze_static_call`) callee signature,
    /// so an empty literal like `let s: Set<int> = {};` still resolves correctly.
    fn lower_collection_literal_call(
        &mut self,
        base: &str,
        type_args: Vec<Type>,
        method: &str,
        args: Vec<ExpressionNode<'a>>,
        ctx: &super::super::AnalyzerContext<'a, '_>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Type, SemanticError> {
        let receiver = self.arena.alloc(ExpressionNode::Identifier(synthetic_token(
            TokenKind::IdentifierToken,
            base,
        )));
        let call = ExpressionNode::MethodCall(
            receiver,
            synthetic_token(TokenKind::IdentifierToken, method),
            Some(type_args),
            args,
        );
        self.analyze_expression(&call, ctx.parent_function, ctx.symbol_table, diagnostics)
    }

    /// When the surrounding context expects `double`, retarget unsuffixed float/int literals so
    /// `let x: double = 3.14` and `Math` double overloads don't require a `d` suffix. Explicit
    /// `f`/`d`/`L`/… suffixes are already classified by the parser; bare decimals arrive as
    /// `Float`, bare integers as `Integer`.
    fn retarget_numeric_literal(lit: &Type, expected: Option<&Type>) -> Type {
        match (expected, lit) {
            (Some(Type::Double(_)), Type::Float(t) | Type::Integer(t)) => Type::Double(t.clone()),
            (Some(Type::Float(_)), Type::Integer(t)) => Type::Float(t.clone()),
            (Some(Type::UInt(_)), Type::Integer(t) | Type::UInt(t)) => Type::UInt(t.clone()),
            (Some(Type::Long(_)), Type::Integer(t)) => Type::Long(t.clone()),
            (Some(Type::ULong(_)), Type::Integer(t)) => Type::ULong(t.clone()),
            (Some(Type::ISize(_)), Type::Integer(t)) => Type::ISize(t.clone()),
            (Some(Type::USize(_)), Type::Integer(t) | Type::UInt(t)) => Type::USize(t.clone()),
            (Some(Type::Byte(_)), Type::Integer(t)) => Type::Byte(t.clone()),
            _ => lit.clone(),
        }
    }
}
