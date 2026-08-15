//! Expression → WGSL lowering (including scalar coercion).

use super::context::EmitCtx;
use super::ident::escape_wgsl_ident;
use super::ty::{
    cast_wgsl_if_needed, common_arith_wgsl_ty, dream_ty_to_wgsl,
    infer_wgsl_ty, is_mat_wgsl, is_vec_wgsl, receiver_wgsl_ty,
};
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::number::{
    numeric_body_is_float, parse_float_literal, parse_int_literal, split_numeric_literal,
};
use dream_syntax::token::token_kind::TokenKind;
use dream_text::text_span::TextSpan;

/// Emit `expr`, inserting a WGSL constructor cast when the inferred type differs from `want`.
pub(super) fn coerce_expr_to_wgsl_ty(expr: &ExpressionNode<'_>, want: &str, ctx: &EmitCtx<'_>) -> String {
    let got = infer_wgsl_ty(expr, ctx);
    let rendered = emit_expr(expr, ctx);
    cast_wgsl_if_needed(rendered, &got, want)
}

pub(super) fn emit_call(name: &str, args: &[ExpressionNode<'_>], ctx: &EmitCtx<'_>) -> String {
    let coerce_all = |want: &str| -> Vec<String> {
        args.iter()
            .map(|a| coerce_expr_to_wgsl_ty(a, want, ctx))
            .collect()
    };
    match name {
        "workgroup_barrier" => "workgroupBarrier()".into(),
        "storage_barrier" => "storageBarrier()".into(),
        "atomic_load" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            let buf = args_s.first().cloned().unwrap_or_else(|| "buf".into());
            let idx = args
                .get(1)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            format!("atomicLoad(&{buf}[u32({idx})])")
        }
        "atomic_store" | "atomic_add" | "atomic_exchange" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            let buf = args_s.first().cloned().unwrap_or_else(|| "buf".into());
            let idx = args
                .get(1)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            let val = args
                .get(2)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            let op = match name {
                "atomic_store" => "atomicStore",
                "atomic_add" => "atomicAdd",
                _ => "atomicExchange",
            };
            format!("{op}(&{buf}[u32({idx})], {val})")
        }
        "texture_load" => {
            let tex = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "tex".into());
            let x = args
                .get(1)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            let y = args
                .get(2)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            format!("textureLoad({tex}, vec2<i32>({x}, {y}), 0)")
        }
        "texture_store" => {
            let tex = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "tex".into());
            let x = args
                .get(1)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            let y = args
                .get(2)
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            let r = args
                .get(3)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            let g = args
                .get(4)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            let b = args
                .get(5)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            let a = args
                .get(6)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "1.0".into());
            format!("textureStore({tex}, vec2<i32>({x}, {y}), vec4<f32>({r}, {g}, {b}, {a}))")
        }
        "texture_sample_level" => {
            let tex = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "tex".into());
            let samp = args
                .get(1)
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "samp".into());
            let u = args
                .get(2)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            let v = args
                .get(3)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            let level = args
                .get(4)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            format!("textureSampleLevel({tex}, {samp}, vec2<f32>({u}, {v}), {level})")
        }
        "texture_sample" => {
            let tex = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "tex".into());
            let samp = args
                .get(1)
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "samp".into());
            let u = args
                .get(2)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            let v = args
                .get(3)
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            format!("textureSample({tex}, {samp}, vec2<f32>({u}, {v}))")
        }
        "of" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            let tys: Vec<String> = args.iter().map(|a| infer_wgsl_ty(a, ctx)).collect();
            if !tys.is_empty() && tys.iter().all(|t| t.starts_with("vec")) {
                let joined = args_s.join(", ");
                match args.len() {
                    2 => format!("mat2x2<f32>({joined})"),
                    3 => format!("mat3x3<f32>({joined})"),
                    4 => format!("mat4x4<f32>({joined})"),
                    n => {
                        ctx.report_error(
                            format!(
                                "GPU shader '{}' matrix constructor expects 2, 3, or 4 column vectors, found {n}",
                                ctx.kernel
                            ),
                            None,
                        );
                        format!("mat4x4<f32>({joined})")
                    }
                }
            } else {
                let n = args.len();
                let args_s = coerce_all("f32");
                let joined = args_s.join(", ");
                match n {
                    2 => format!("vec2<f32>({joined})"),
                    3 => format!("vec3<f32>({joined})"),
                    4 => format!("vec4<f32>({joined})"),
                    n => {
                        ctx.report_error(
                            format!(
                                "GPU shader '{}' vector constructor expects 2, 3, or 4 components, found {n}",
                                ctx.kernel
                            ),
                            None,
                        );
                        format!("vec4<f32>({joined})")
                    }
                }
            }
        }
        "identity" => {
            ctx.report_error(
                format!(
                    "GPU shader '{}' cannot lower a free identity() call; use GpuMat2/3/4.identity()",
                    ctx.kernel
                ),
                None,
            );
            "mat4x4<f32>(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)".into()
        }
        "mul" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "({} * {})",
                args_s.first().cloned().unwrap_or_else(|| "m".into()),
                args_s.get(1).cloned().unwrap_or_else(|| "v".into())
            )
        }
        "transpose" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "transpose({})",
                args_s.first().cloned().unwrap_or_else(|| "m".into())
            )
        }
        "splat" => {
            ctx.report_error(
                format!(
                    "GPU shader '{}' cannot lower a free splat() call; use GpuVec2/3/4.splat()",
                    ctx.kernel
                ),
                None,
            );
            let s = args
                .first()
                .map(|a| coerce_expr_to_wgsl_ty(a, "f32", ctx))
                .unwrap_or_else(|| "0.0".into());
            format!("vec3<f32>({s})")
        }
        "min" | "max" | "abs" | "clamp" | "sqrt" | "floor" | "ceil" | "fract" | "sin" | "cos" | "tan"
        | "asin" | "acos" | "atan" | "atan2" | "normalize" | "length" | "dot" | "cross"
        | "reflect" | "mix" | "pow" | "exp" | "log" | "sign" | "saturate" | "step" | "smoothstep"
        | "fma" | "inversesqrt" => {
            let arg_tys: Vec<String> = args.iter().map(|a| infer_wgsl_ty(a, ctx)).collect();
            let any_vec = arg_tys.iter().any(|t| is_vec_wgsl(t));
            let vec_ty = arg_tys.iter().find(|t| is_vec_wgsl(t)).cloned();
            let args_s: Vec<String> = if any_vec {
                args.iter()
                    .zip(arg_tys.iter())
                    .map(|(a, ty)| {
                        let rendered = emit_expr(a, ctx);
                        if *ty == "f32" {
                            if let Some(vty) = vec_ty.as_deref() {
                                if name == "mix" {
                                    // WGSL `mix(vec, vec, f32)` keeps a scalar factor.
                                    return rendered;
                                }
                                return format!("{vty}({rendered})");
                            }
                        }
                        rendered
                    })
                    .collect()
            } else {
                coerce_all("f32")
            };
            match name {
                "min" => format!(
                    "min({}, {})",
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(1).cloned().unwrap_or_else(|| "0.0".into())
                ),
                "max" => format!(
                    "max({}, {})",
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(1).cloned().unwrap_or_else(|| "0.0".into())
                ),
                "abs" => format!(
                    "abs({})",
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into())
                ),
                "clamp" | "smoothstep" | "fma" => format!(
                    "{}({}, {}, {})",
                    name,
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(1).cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(2).cloned().unwrap_or_else(|| "0.0".into())
                ),
                "saturate" => {
                    if let Some(vty) = vec_ty.as_deref() {
                        format!(
                            "clamp({}, {vty}(0.0), {vty}(1.0))",
                            args_s.first().cloned().unwrap_or_else(|| "0.0".into())
                        )
                    } else {
                        format!(
                            "clamp({}, 0.0, 1.0)",
                            args_s.first().cloned().unwrap_or_else(|| "0.0".into())
                        )
                    }
                }
                "atan2" | "step" | "pow" => format!(
                    "{}({}, {})",
                    name,
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(1).cloned().unwrap_or_else(|| "0.0".into())
                ),
                "mix" => format!(
                    "mix({}, {}, {})",
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(1).cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(2).cloned().unwrap_or_else(|| "0.0".into())
                ),
                "normalize" | "length" | "cross" | "reflect" | "dot" | "exp" | "log" | "sign"
                | "inversesqrt" => {
                    format!("{}({})", name, args_s.join(", "))
                }
                other => format!(
                    "{}({})",
                    other,
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into())
                ),
            }
        }
        other => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!("{}({})", escape_wgsl_ident(other), args_s.join(", "))
        }
    }
}

fn reject_gpu_type_args(type_args: &Option<Vec<Type>>, span: Option<TextSpan>, ctx: &EmitCtx<'_>) {
    if type_args.as_ref().is_some_and(|a| !a.is_empty()) {
        ctx.report_error(
            format!(
                "GPU shader '{}' does not support generic type arguments on calls",
                ctx.kernel
            ),
            span,
        );
    }
}

fn mat_identity_wgsl(obj: &ExpressionNode<'_>, ctx: &EmitCtx<'_>) -> String {
    let name = match obj {
        ExpressionNode::Identifier(n) => n.text.as_str(),
        _ => {
            return ctx.unsupported_expr("identity() on a non-type receiver", obj.position());
        }
    };
    match name {
        "GpuMat2" => "mat2x2<f32>(1.0, 0.0, 0.0, 1.0)".into(),
        "GpuMat3" => {
            "mat3x3<f32>(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0)".into()
        }
        "GpuMat4" => {
            "mat4x4<f32>(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)".into()
        }
        other => {
            ctx.report_error(
                format!(
                    "GPU shader '{}' cannot emit identity() for '{other}'",
                    ctx.kernel
                ),
                obj.position(),
            );
            "mat4x4<f32>(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)".into()
        }
    }
}

fn splat_wgsl(obj: &ExpressionNode<'_>, arg: &ExpressionNode<'_>, ctx: &EmitCtx<'_>) -> String {
    let s = coerce_expr_to_wgsl_ty(arg, "f32", ctx);
    match receiver_wgsl_ty(obj) {
        Some(ty) => format!("{ty}({s})"),
        None => {
            ctx.report_error(
                format!(
                    "GPU shader '{}' cannot emit splat() for a non-GpuVec receiver",
                    ctx.kernel
                ),
                obj.position(),
            );
            format!("vec3<f32>({s})")
        }
    }
}

fn literal_text(ty: &Type, ctx: &EmitCtx<'_>) -> String {
    match ty {
        Type::Boolean(t) => t.text.clone(),
        Type::Char(t) => ctx.unsupported_expr("character literal", Some(t.position)),
        Type::String(t) => ctx.unsupported_expr("string literal", Some(t.position)),
        Type::Integer(t)
        | Type::Long(t)
        | Type::UInt(t)
        | Type::ULong(t)
        | Type::Byte(t)
        | Type::Float(t)
        | Type::Double(t) => {
            let raw = t.text.as_str();
            let (body, _) = split_numeric_literal(raw).unwrap_or((raw, ""));
            let is_float = matches!(ty, Type::Float(_) | Type::Double(_))
                || numeric_body_is_float(body);
            if is_float {
                match parse_float_literal(body) {
                    Some(v) => {
                        let mut s = format!("{v}");
                        if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                            s.push_str(".0");
                        }
                        s
                    }
                    None => {
                        ctx.report_error(
                            format!(
                                "GPU shader '{}' has an unparseable float literal '{raw}'",
                                ctx.kernel
                            ),
                            Some(t.position),
                        );
                        "0.0".into()
                    }
                }
            } else {
                match parse_int_literal(body) {
                    Some(v) => v.to_string(),
                    None => {
                        ctx.report_error(
                            format!(
                                "GPU shader '{}' has an unparseable integer literal '{raw}'",
                                ctx.kernel
                            ),
                            Some(t.position),
                        );
                        "0".into()
                    }
                }
            }
        }
        _ => ctx.unsupported_expr("literal", ty.get_span()),
    }
}

pub(super) fn emit_expr(expr: &ExpressionNode<'_>, ctx: &EmitCtx<'_>) -> String {
    match expr {
        ExpressionNode::Literal(ty) => literal_text(ty, ctx),
        ExpressionNode::Identifier(t) => ctx.rewrite_ident(&t.text),
        ExpressionNode::Binary(l, op, r) => {
            let op_s = match op.kind {
                TokenKind::PlusToken => "+",
                TokenKind::MinusToken => "-",
                TokenKind::StarToken => "*",
                TokenKind::SlashToken => "/",
                TokenKind::ModulusToken => "%",
                TokenKind::EqualEqualToken => "==",
                TokenKind::NotEqualToken => "!=",
                TokenKind::SmallerThanToken => "<",
                TokenKind::SmallerThanEqualToken => "<=",
                TokenKind::GreaterThanToken => ">",
                TokenKind::GreaterThanEqualToken => ">=",
                TokenKind::AmpersandAmpersandToken => "&&",
                TokenKind::PipePipeToken => "||",
                TokenKind::BitWiseAmpersandToken => "&",
                TokenKind::BitWisePipeToken => "|",
                TokenKind::BitWiseXorToken => "^",
                _ => {
                    ctx.report_error(
                        format!(
                            "GPU shader '{}' does not support operator '{}'",
                            ctx.kernel, op.text
                        ),
                        Some(op.position),
                    );
                    "+"
                }
            };
            let ls = emit_expr(l, ctx);
            let rs = emit_expr(r, ctx);
            // Logical ops stay bool/bool; everything else needs matching WGSL scalar types.
            if matches!(
                op.kind,
                TokenKind::AmpersandAmpersandToken | TokenKind::PipePipeToken
            ) {
                return format!("({ls} {op_s} {rs})");
            }
            let lt = infer_wgsl_ty(l, ctx);
            let rt = infer_wgsl_ty(r, ctx);
            let common = common_arith_wgsl_ty(&lt, &rt);
            // WGSL already allows `vecN op f32` / `f32 op vecN` without constructor casts.
            if ((is_vec_wgsl(&lt) || is_mat_wgsl(&lt)) && rt == "f32")
                || (is_vec_wgsl(&rt) && lt == "f32")
                || (is_mat_wgsl(&lt) && is_vec_wgsl(&rt))
                || (is_mat_wgsl(&lt) && is_mat_wgsl(&rt))
            {
                return format!("({ls} {op_s} {rs})");
            }
            format!(
                "({} {} {})",
                cast_wgsl_if_needed(ls, &lt, &common),
                op_s,
                cast_wgsl_if_needed(rs, &rt, &common)
            )
        }
        ExpressionNode::Unary(op, e) => {
            let op_s = match op.kind {
                TokenKind::MinusToken => "-",
                TokenKind::PlusToken => "+",
                TokenKind::BangToken => "!",
                TokenKind::TildeToken => "~",
                _ => {
                    ctx.report_error(
                        format!(
                            "GPU shader '{}' does not support unary operator '{}'",
                            ctx.kernel, op.text
                        ),
                        Some(op.position),
                    );
                    "-"
                }
            };
            format!("({}{})", op_s, emit_expr(e, ctx))
        }
        ExpressionNode::IncDec { op, .. } => ctx.unsupported_expr(
            "nested increment/decrement (write `x = x + 1` instead)",
            Some(op.position),
        ),
        ExpressionNode::Parenthesized(_, e) => format!("({})", emit_expr(e, ctx)),
        ExpressionNode::IndexAccess(arr, idx) => {
            let arr_s = emit_expr(arr, ctx);
            let idx_s = coerce_expr_to_wgsl_ty(idx, "i32", ctx);
            let atomic = matches!(arr, ExpressionNode::Identifier(n) if ctx.is_atomic_buf(&n.text));
            if atomic {
                format!("atomicLoad(&{arr_s}[u32({idx_s})])")
            } else {
                format!("{arr_s}[u32({idx_s})]")
            }
        }
        ExpressionNode::MemberAccess(obj, member) => {
            let base = emit_expr(obj, ctx);
            if member.text == "length" {
                format!("i32(arrayLength(&{}))", base)
            } else {
                format!("{}.{}", base, escape_wgsl_ident(&member.text))
            }
        }
        ExpressionNode::FunctionCall(name, type_args, args) => {
            reject_gpu_type_args(type_args, Some(name.position), ctx);
            emit_call(&name.text, args, ctx)
        }
        ExpressionNode::MethodCall(obj, method, type_args, args) => {
            reject_gpu_type_args(type_args, Some(method.position), ctx);
            if method.text == "length" && args.is_empty() {
                format!("i32(arrayLength(&{}))", emit_expr(obj, ctx))
            } else if method.text == "identity" && args.is_empty() {
                mat_identity_wgsl(obj, ctx)
            } else if method.text == "splat" && args.len() == 1 {
                splat_wgsl(obj, &args[0], ctx)
            } else {
                emit_call(&method.text, args, ctx)
            }
        }
        ExpressionNode::Call(callee, type_args, args) => {
            reject_gpu_type_args(type_args, callee.position(), ctx);
            match &**callee {
                ExpressionNode::Identifier(n) => emit_call(&n.text, args, ctx),
                ExpressionNode::MemberAccess(obj, method) => {
                    if method.text == "length" && args.is_empty() {
                        format!("i32(arrayLength(&{}))", emit_expr(obj, ctx))
                    } else if method.text == "identity" && args.is_empty() {
                        mat_identity_wgsl(obj, ctx)
                    } else if method.text == "splat" && args.len() == 1 {
                        splat_wgsl(obj, &args[0], ctx)
                    } else {
                        emit_call(&method.text, args, ctx)
                    }
                }
                _ => ctx.unsupported_expr("call whose callee is not a name or member", expr.position()),
            }
        }
        ExpressionNode::Ternary(c, t, e) => {
            let tt = infer_wgsl_ty(t, ctx);
            let et = infer_wgsl_ty(e, ctx);
            let common = common_arith_wgsl_ty(&tt, &et);
            format!(
                "select({}, {}, {})",
                coerce_expr_to_wgsl_ty(e, &common, ctx),
                coerce_expr_to_wgsl_ty(t, &common, ctx),
                emit_expr(c, ctx)
            )
        }
        ExpressionNode::Cast(_, ty, e) => {
            let wty = dream_ty_to_wgsl(ty);
            coerce_expr_to_wgsl_ty(e, &wty, ctx)
        }
        ExpressionNode::SizeOf(_, ty) => format!("{}", gpu_sizeof_bytes(ty, ctx)),
        ExpressionNode::NameOf(_, _) => "0".into(),
        ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
            emit_expr(inner, ctx)
        }
        ExpressionNode::ArrayLiteral(tok, _) => {
            ctx.unsupported_expr("array literal", Some(tok.position))
        }
        ExpressionNode::TupleLiteral(tok, _) => {
            ctx.unsupported_expr("tuple literal", Some(tok.position))
        }
        ExpressionNode::SetLiteral(tok, _) => {
            ctx.unsupported_expr("set literal", Some(tok.position))
        }
        ExpressionNode::MapLiteral(tok, _) => {
            ctx.unsupported_expr("map literal", Some(tok.position))
        }
        ExpressionNode::IsExpression(e, _, _) => {
            ctx.unsupported_expr("`is` type check", e.position())
        }
        ExpressionNode::Await(tok, _) => ctx.unsupported_expr("await", Some(tok.position)),
        ExpressionNode::Try(e) => ctx.unsupported_expr("`?` try operator", e.position()),
        ExpressionNode::Switch(tok, _, _) => {
            ctx.unsupported_expr("pattern switch", Some(tok.position))
        }
        ExpressionNode::Lambda(l) => ctx.unsupported_expr("lambda", Some(l.start_span())),
        ExpressionNode::SyntaxBlock(block) => {
            ctx.unsupported_expr("syntax block", Some(block.name.position))
        }
    }
}

/// Dream ABI byte size for `sizeof(T)` inside WGSL emission (matches host `sizeof` for scalars /
/// GPU builtins; user value structs sum field sizes from the shader field map).
fn gpu_sizeof_bytes(ty: &Type, ctx: &EmitCtx<'_>) -> u32 {
    match ty {
        Type::Struct(tok, _) => {
            if let Some(fields) = ctx.struct_fields.get(&tok.text) {
                let mut size = 0u32;
                for wgsl_ty in fields.values() {
                    size = size.saturating_add(wgsl_type_byte_size(wgsl_ty));
                }
                return size.max(1);
            }
            dream_types::value_size_align(&tok.text).0 as u32
        }
        Type::Array(_) => 4,
        _ => {
            let name = ty.get_type();
            dream_types::value_size_align(&name).0 as u32
        }
    }
}

fn wgsl_type_byte_size(wgsl_ty: &str) -> u32 {
    match wgsl_ty {
        "bool" | "i32" | "u32" | "f32" => 4,
        "vec2<f32>" | "vec2<i32>" | "vec2<u32>" => 8,
        "vec3<f32>" | "vec3<i32>" | "vec3<u32>" => 12,
        "vec4<f32>" | "vec4<i32>" | "vec4<u32>" => 16,
        "mat2x2<f32>" => 16,
        "mat3x3<f32>" => 48,
        "mat4x4<f32>" => 64,
        _ => 4,
    }
}
