//! Expression → WGSL lowering (including scalar coercion).

use super::context::EmitCtx;
use super::ident::escape_wgsl_ident;
use super::ty::{
    cast_wgsl_if_needed, common_numeric_wgsl_ty, dream_ty_to_wgsl, infer_wgsl_ty,
};
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::token::token_kind::TokenKind;

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
                    _ => format!("mat4x4<f32>({joined})"),
                }
            } else {
                let n = args.len();
                let args_s = coerce_all("f32");
                let joined = args_s.join(", ");
                match n {
                    2 => format!("vec2<f32>({joined})"),
                    3 => format!("vec3<f32>({joined})"),
                    4 => format!("vec4<f32>({joined})"),
                    _ => format!("vec4<f32>({joined})"),
                }
            }
        }
        "identity" => {
            // GpuMatN.identity() — arity inferred from call site is lost; default mat4.
            // Prefer explicit column constructors when size matters; identity is sugar.
            "mat4x4<f32>(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0)".into()
        }
        "mul2" | "mul3" | "mul4" | "matmul4" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "({} * {})",
                args_s.first().cloned().unwrap_or_else(|| "m".into()),
                args_s.get(1).cloned().unwrap_or_else(|| "v".into())
            )
        }
        "transpose4" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "transpose({})",
                args_s.first().cloned().unwrap_or_else(|| "m".into())
            )
        }
        "length2" | "length4" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "length({})",
                args_s.first().cloned().unwrap_or_else(|| "v".into())
            )
        }
        "normalize2" | "normalize4" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "normalize({})",
                args_s.first().cloned().unwrap_or_else(|| "v".into())
            )
        }
        "dot2" | "dot4" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            format!(
                "dot({}, {})",
                args_s.first().cloned().unwrap_or_else(|| "a".into()),
                args_s.get(1).cloned().unwrap_or_else(|| "b".into())
            )
        }
        "min" | "max" | "abs" | "clamp" | "sqrt" | "floor" | "ceil" | "fract" | "sin" | "cos" | "tan"
        | "asin" | "acos" | "atan" | "atan2" | "normalize" | "length" | "dot" | "cross"
        | "reflect" | "mix" | "pow" | "exp" | "log" | "sign" | "saturate" | "step" | "smoothstep"
        | "fma" | "inversesqrt" => {
            let args_s: Vec<String> = args.iter().map(|a| emit_expr(a, ctx)).collect();
            let scalar = matches!(
                name,
                "min" | "max" | "abs" | "clamp" | "sqrt" | "floor" | "ceil" | "fract" | "sin"
                    | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2" | "mix" | "pow" | "exp"
                    | "log" | "sign" | "saturate" | "step" | "smoothstep" | "fma" | "inversesqrt"
            );
            let args_s = if scalar {
                coerce_all("f32")
            } else {
                args_s
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
                "saturate" => format!(
                    "clamp({}, 0.0, 1.0)",
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into())
                ),
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

fn literal_text(ty: &Type) -> String {
    match ty {
        Type::Integer(t)
        | Type::Float(t)
        | Type::Double(t)
        | Type::Long(t)
        | Type::UInt(t)
        | Type::ULong(t)
        | Type::Byte(t)
        | Type::Boolean(t)
        | Type::Char(t)
        | Type::String(t) => {
            let mut s = t.text.clone();
            if matches!(ty, Type::Float(_) | Type::Double(_))
                && !s.contains('.')
                && !s.contains('e')
                && !s.contains('E')
            {
                s.push_str(".0");
            }
            s
        }
        _ => "0".into(),
    }
}

pub(super) fn emit_expr(expr: &ExpressionNode<'_>, ctx: &EmitCtx<'_>) -> String {
    match expr {
        ExpressionNode::Literal(ty) => literal_text(ty),
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
                _ => "+",
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
            let common = common_numeric_wgsl_ty(&lt, &rt);
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
                TokenKind::BangToken => "!",
                TokenKind::TildeToken => "~",
                _ => "-",
            };
            format!("({}{})", op_s, emit_expr(e, ctx))
        }
        // Statement `i++`/`++i` are desugared to Assignment before emission. Value-producing
        // forms in kernels are expanded in `emit_stmt` for declarations; nested uses fall back
        // to the place (side effect must be written as `i = i + 1` in kernels).
        ExpressionNode::IncDec { target, .. } => emit_expr(target, ctx),
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
        ExpressionNode::FunctionCall(name, _, args) => emit_call(&name.text, args, ctx),
        ExpressionNode::MethodCall(obj, method, _, args) => {
            // Zero-arg `.length()` on a buffer → arrayLength; `GpuMath.length(vec)` has args.
            if method.text == "length" && args.is_empty() {
                format!("i32(arrayLength(&{}))", emit_expr(obj, ctx))
            } else {
                emit_call(&method.text, args, ctx)
            }
        }
        ExpressionNode::Call(callee, _, args) => match &**callee {
            ExpressionNode::Identifier(n) => emit_call(&n.text, args, ctx),
            ExpressionNode::MemberAccess(obj, method) => {
                if method.text == "length" && args.is_empty() {
                    format!("i32(arrayLength(&{}))", emit_expr(obj, ctx))
                } else {
                    emit_call(&method.text, args, ctx)
                }
            }
            _ => "0".into(),
        },
        ExpressionNode::Ternary(c, t, e) => {
            let tt = infer_wgsl_ty(t, ctx);
            let et = infer_wgsl_ty(e, ctx);
            let common = common_numeric_wgsl_ty(&tt, &et);
            format!(
                "select({}, {}, {})",
                coerce_expr_to_wgsl_ty(e, &common, ctx),
                coerce_expr_to_wgsl_ty(t, &common, ctx),
                emit_expr(c, ctx)
            )
        },
        ExpressionNode::Cast(_, ty, e) => {
            // Coerce once — wrapping again would emit `f32(f32(x))` / `i32(i32(x))`.
            let wty = dream_ty_to_wgsl(ty);
            coerce_expr_to_wgsl_ty(e, &wty, ctx)
        }
        ExpressionNode::NamedArg(_, inner) | ExpressionNode::RefArgument(_, inner) => {
            emit_expr(inner, ctx)
        }
        _ => "0".into(),
    }
}
