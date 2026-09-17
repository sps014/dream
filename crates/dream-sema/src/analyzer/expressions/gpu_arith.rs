//! WGSL-shaped arithmetic on `GpuVecN` / `GpuMatN`: `v+w`, `v*s`, `s*v`, `-v`, `m*v`.
//! Shader bodies skip HIR; CPU uses the matching instance methods on the stdlib structs.

use super::*;
use crate::errors::SemanticError;
use dream_hir::HExpr;
use dream_syntax::nodes::Type;
use dream_syntax::token::syntax_token::SyntaxToken;
use dream_syntax::token::token_kind::TokenKind;
use dream_types::method_fn;

impl<'a> Analyzer<'a> {
    pub(super) fn gpu_struct_name(ty: &Type) -> Option<&str> {
        match ty {
            Type::Struct(tok, args) if args.as_ref().is_none_or(|a| a.is_empty()) => {
                Some(tok.text.as_str())
            }
            _ => None,
        }
    }

    pub(super) fn is_gpu_vec(ty: &Type) -> bool {
        matches!(
            Self::gpu_struct_name(ty),
            Some("GpuVec2" | "GpuVec3" | "GpuVec4")
        )
    }

    fn is_gpu_float(ty: &Type) -> bool {
        matches!(ty, Type::Float(_) | Type::Double(_))
    }

    fn gpu_vec_rank(name: &str) -> Option<u8> {
        match name {
            "GpuVec2" => Some(2),
            "GpuVec3" => Some(3),
            "GpuVec4" => Some(4),
            _ => None,
        }
    }

    fn gpu_mat_rank(name: &str) -> Option<u8> {
        match name {
            "GpuMat2" => Some(2),
            "GpuMat3" => Some(3),
            "GpuMat4" => Some(4),
            _ => None,
        }
    }

    /// Types a multi-component swizzle read (`v.xyz`, `color.rgb`, `uv.yx`) on a `GpuVecN`.
    ///
    /// `None` means the member is not a swizzle and the caller should carry on to its own
    /// "no such field" reporting. Single-component reads are also `None`: `v.x` is a real struct
    /// field and resolves before this is ever reached.
    ///
    /// Shader bodies are emitted from the AST, where this stays a member access and becomes a
    /// native WGSL swizzle. Only the CPU path takes the `GpuVecN.of(...)` desugar below, which
    /// mentions the receiver once per component — hence the restriction to receivers that are
    /// free to re-read.
    pub(super) fn try_gpu_swizzle(
        &mut self,
        obj: &'a ExpressionNode<'a>,
        obj_type: &Type,
        member: &SyntaxToken,
        parent_function: &FunctionNode<'a>,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) -> Option<Result<Type, SemanticError>> {
        let name = Self::gpu_struct_name(obj_type)?;
        let arity = Self::gpu_vec_rank(name)? as usize;
        if !dream_abi::gpu_swizzle::is_component_name(&member.text) {
            return None;
        }

        let Some(comps) = dream_abi::gpu_swizzle::components(&member.text, arity) else {
            // Component letters that do not resolve: either past the end of this vector, or the
            // `xyzw` and `rgba` spellings mixed in one name. Worth its own message, since the
            // caller's "field not found" would send the reader looking for a field.
            self.hir_none();
            return Some(Err(report(
                diagnostics,
                format!(
                    "'{}' is not a valid swizzle of {name}; components must all be from 'xyzw' or \
                     all from 'rgba', and must fit in the {arity} components of {name}",
                    member.text
                ),
                Some(member.position),
            )));
        };
        if comps.len() == 1 {
            return None;
        }

        if !self.current_function_is_gpu && !Self::is_rereadable(obj) {
            self.hir_none();
            return Some(Err(report(
                diagnostics,
                format!(
                    "swizzle '.{}' needs a receiver that can be read more than once on the CPU; \
                     bind the value to a local first",
                    member.text
                ),
                Some(member.position),
            )));
        }

        let result = format!("GpuVec{}", comps.len());
        let args: Vec<ExpressionNode<'a>> = comps
            .iter()
            .map(|&i| {
                let field = synthetic_token(TokenKind::IdentifierToken, Self::VEC_FIELDS[i]);
                ExpressionNode::MemberAccess(obj, field)
            })
            .collect();
        let recv = &*self
            .arena
            .alloc(ExpressionNode::Identifier(synthetic_token(
                TokenKind::IdentifierToken,
                &result,
            )));
        let of = synthetic_token(TokenKind::IdentifierToken, "of");
        let call = ExpressionNode::MethodCall(recv, of, None, args);
        Some(self.analyze_expression(&call, parent_function, symbol_table, diagnostics))
    }

    /// Storage field name for each vector component, in order.
    const VEC_FIELDS: [&'static str; 4] = ["x", "y", "z", "w"];

    /// Whether `expr` can be evaluated repeatedly with no extra cost or effect.
    ///
    /// Reads of locals and walks of their fields qualify; anything that could call code or index
    /// does not, because the swizzle desugar would duplicate it once per component.
    fn is_rereadable(expr: &ExpressionNode<'_>) -> bool {
        match expr {
            ExpressionNode::Identifier(_) => true,
            ExpressionNode::MemberAccess(inner, _) => Self::is_rereadable(inner),
            _ => false,
        }
    }

    /// When `op` is WGSL-legal on GPU vectors/matrices, types the result and lowers CPU HIR to
    /// stdlib methods. `None` means this is not GPU arithmetic (fall through to numeric rules).
    pub(super) fn try_gpu_binary(
        &mut self,
        left: &Type,
        opr: &SyntaxToken,
        right: &Type,
        left_hir: Option<HExpr>,
        right_hir: Option<HExpr>,
    ) -> Option<Result<Type, SemanticError>> {
        if left.is_unknown() || right.is_unknown() {
            return None;
        }
        let ln = Self::gpu_struct_name(left);
        let rn = Self::gpu_struct_name(right);
        let arith = matches!(
            opr.kind,
            TokenKind::PlusToken
                | TokenKind::MinusToken
                | TokenKind::StarToken
                | TokenKind::SlashToken
        );
        if !arith {
            return None;
        }

        // `GpuMatN * GpuVecN` and `GpuMatN * GpuMatN`.
        if opr.kind == TokenKind::StarToken {
            if let (Some(lname), Some(lm), Some(rv_rank)) = (
                ln,
                ln.and_then(Self::gpu_mat_rank),
                rn.and_then(Self::gpu_vec_rank),
            ) {
                if lm == rv_rank {
                    let method = method_fn(lname, "mul");
                    self.hir_set_method_call(left_hir, &method, vec![right_hir], right);
                    return Some(Ok(right.clone()));
                }
            }
            if let (Some(lname), Some(lm), Some(rm)) = (
                ln,
                ln.and_then(Self::gpu_mat_rank),
                rn.and_then(Self::gpu_mat_rank),
            ) {
                if lm == rm {
                    let method = method_fn(lname, "mul_mat");
                    self.hir_set_method_call(left_hir, &method, vec![right_hir], left);
                    return Some(Ok(left.clone()));
                }
            }
        }

        let lv = Self::is_gpu_vec(left);
        let rv = Self::is_gpu_vec(right);
        if !lv && !rv {
            return None;
        }

        if lv && rv {
            if ln != rn {
                return None;
            }
            let lname = ln?;
            let method = match opr.kind {
                TokenKind::PlusToken => "add",
                TokenKind::MinusToken => "sub",
                TokenKind::StarToken => "mul",
                TokenKind::SlashToken => "div",
                _ => return None,
            };
            let mangled = method_fn(lname, method);
            self.hir_set_method_call(left_hir, &mangled, vec![right_hir], left);
            return Some(Ok(left.clone()));
        }

        // Vec ⊗ float or float ⊗ vec (WGSL allows the scalar on either side).
        let (vec_ty, vec_hir, scalar_hir, method) = if lv && Self::is_gpu_float(right) {
            let method = match opr.kind {
                TokenKind::StarToken => "scale",
                TokenKind::SlashToken => "div_scalar",
                TokenKind::PlusToken => "add_scalar",
                TokenKind::MinusToken => "sub_scalar",
                _ => return None,
            };
            (left, left_hir, right_hir, method)
        } else if rv && Self::is_gpu_float(left) {
            let method = match opr.kind {
                TokenKind::StarToken => "scale",
                TokenKind::PlusToken => "add_scalar",
                TokenKind::SlashToken => "div_from_scalar",
                TokenKind::MinusToken => "sub_from_scalar",
                _ => return None,
            };
            (right, right_hir, left_hir, method)
        } else {
            return None;
        };
        let name = Self::gpu_struct_name(vec_ty)?;
        let mangled = method_fn(name, method);
        self.hir_set_method_call(vec_hir, &mangled, vec![scalar_hir], vec_ty);
        Some(Ok(vec_ty.clone()))
    }

    pub(super) fn try_gpu_unary_neg(&mut self, operand: &Type, operand_hir: Option<HExpr>) -> bool {
        if !Self::is_gpu_vec(operand) {
            return false;
        }
        let Some(name) = Self::gpu_struct_name(operand) else {
            return false;
        };
        let mangled = method_fn(name, "neg");
        self.hir_set_method_call(operand_hir, &mangled, vec![], operand);
        true
    }
}
