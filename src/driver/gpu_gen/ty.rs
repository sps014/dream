use super::context::EmitCtx;
use super::ident::escape_wgsl_ident;
use dream_syntax::nodes::expression::ExpressionNode;
use dream_syntax::nodes::types::Type;
use dream_syntax::token::token_kind::TokenKind;

pub(super) fn dream_ty_to_wgsl(ty: &Type) -> String {
    match ty {
        Type::Float(_) | Type::Double(_) => "f32".into(),
        Type::Integer(_) | Type::Byte(_) => "i32".into(),
        Type::UInt(_) => "u32".into(),
        Type::Boolean(_) => "bool".into(),
        Type::Long(_) => "i32".into(),
        Type::ULong(_) => "u32".into(),
        Type::Struct(tok, _) => match tok.text.as_str() {
            "GpuId3" => "vec3<i32>".into(),
            "GpuVec2" => "vec2<f32>".into(),
            "GpuVec3" => "vec3<f32>".into(),
            "GpuVec4" => "vec4<f32>".into(),
            "GpuMat2" => "mat2x2<f32>".into(),
            "GpuMat3" => "mat3x3<f32>".into(),
            "GpuMat4" => "mat4x4<f32>".into(),
            other => escape_wgsl_ident(other),
        },
        Type::Array(inner) => format!("array<{}>", dream_ty_to_wgsl(inner)),
        _ => "i32".into(),
    }
}

pub(super) fn cast_wgsl_if_needed(rendered: String, got: &str, want: &str) -> String {
    if got == want || want.is_empty() {
        return rendered;
    }
    // Never invent scalar↔array/texture casts — those are emitter/user bugs, not coercions.
    if got.starts_with("array<")
        || want.starts_with("array<")
        || got.contains("texture")
        || want.contains("texture")
        || got == "sampler"
        || want == "sampler"
    {
        return rendered;
    }
    format!("{want}({rendered})")
}

pub(super) fn common_numeric_wgsl_ty(lt: &str, rt: &str) -> String {
    if lt.starts_with("array<") || rt.starts_with("array<") {
        return if lt.starts_with("array<") {
            lt
        } else {
            rt
        }
        .into();
    }
    if lt == "f32" || rt == "f32" {
        "f32".into()
    } else if lt == "u32" && rt == "u32" {
        "u32".into()
    } else if lt == "bool" || rt == "bool" {
        "bool".into()
    } else {
        "i32".into()
    }
}

pub(super) fn is_bool_producing_binop(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::EqualEqualToken
            | TokenKind::NotEqualToken
            | TokenKind::SmallerThanToken
            | TokenKind::SmallerThanEqualToken
            | TokenKind::GreaterThanToken
            | TokenKind::GreaterThanEqualToken
            | TokenKind::AmpersandAmpersandToken
            | TokenKind::PipePipeToken
    )
}

/// WGSL return type for a known GPU builtin / GpuMath call, if any.
pub(super) fn builtin_return_wgsl_ty(name: &str, arg_count: usize) -> Option<&'static str> {
    match name {
        "sqrt" | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2" | "floor" | "ceil"
        | "fract" | "min" | "max" | "abs" | "clamp" | "mix" | "pow" | "exp" | "log" | "sign"
        | "saturate" | "step" | "smoothstep" | "fma" | "inversesqrt" | "length" | "length2"
        | "length4" | "dot" | "dot2" | "dot4" => Some("f32"),
        "normalize" | "cross" | "reflect" => Some("vec3<f32>"),
        "normalize2" => Some("vec2<f32>"),
        "normalize4" => Some("vec4<f32>"),
        "mul2" => Some("vec2<f32>"),
        "mul3" => Some("vec3<f32>"),
        "mul4" => Some("vec4<f32>"),
        "matmul4" | "transpose4" | "identity" => Some("mat4x4<f32>"),
        "atomic_load" | "atomic_add" | "atomic_exchange" => Some("i32"),
        "texture_load" | "texture_sample_level" => Some("f32"),
        "texture_sample" => Some("vec4<f32>"),
        "of" => Some(match arg_count {
            2 => "vec2<f32>",
            3 => "vec3<f32>",
            _ => "vec4<f32>",
        }),
        _ => None,
    }
}

/// Best-effort WGSL type for unannotated `let` bindings (casts/literals/float ops).
pub(super) fn infer_wgsl_ty(expr: &ExpressionNode<'_>, ctx: &EmitCtx<'_>) -> String {
    match expr {
        ExpressionNode::Cast(_, ty, _) | ExpressionNode::Literal(ty) => dream_ty_to_wgsl(ty),
        ExpressionNode::SizeOf(_, _) => "i32".into(),
        ExpressionNode::Parenthesized(_, inner)
        | ExpressionNode::NamedArg(_, inner)
        | ExpressionNode::RefArgument(_, inner)
        | ExpressionNode::IncDec { target: inner, .. } => infer_wgsl_ty(inner, ctx),
        ExpressionNode::Unary(op, inner) => {
            if op.kind == TokenKind::BangToken {
                "bool".into()
            } else {
                infer_wgsl_ty(inner, ctx)
            }
        }
        ExpressionNode::Binary(l, op, r) => {
            if is_bool_producing_binop(op.kind) {
                return "bool".into();
            }
            let lt = infer_wgsl_ty(l, ctx);
            let rt = infer_wgsl_ty(r, ctx);
            common_numeric_wgsl_ty(&lt, &rt)
        }
        ExpressionNode::Ternary(_, t, e) => {
            let tt = infer_wgsl_ty(t, ctx);
            let et = infer_wgsl_ty(e, ctx);
            common_numeric_wgsl_ty(&tt, &et)
        }
        ExpressionNode::IndexAccess(arr, _) => {
            if let ExpressionNode::Identifier(name) = &**arr {
                if let Some(t) = ctx.lookup_local(&name.text) {
                    if let Some(inner) = t.strip_prefix("array<").and_then(|s| s.strip_suffix('>')) {
                        return inner.to_string();
                    }
                    return t;
                }
                if let Some(b) = ctx.binding(&name.text) {
                    if b.kind == "storage" {
                        return b.wgsl_ty.clone();
                    }
                }
            }
            "f32".into()
        }
        ExpressionNode::MemberAccess(obj, member) => {
            // Buffer/array `.length` property (no call args). Vector length is `GpuMath.length(...)`.
            if member.text == "length" {
                return "i32".into();
            }
            let base = infer_wgsl_ty(obj, ctx);
            if base.starts_with("vec") {
                // .x/.y/.z/.w of vecN → component type
                if base.contains("f32") {
                    return "f32".into();
                }
                if base.contains("u32") {
                    return "u32".into();
                }
                return "i32".into();
            }
            if let Some(ft) = ctx.lookup_struct_field(&base, &member.text) {
                return ft;
            }
            "i32".into()
        },
        ExpressionNode::Identifier(name) => {
            if let Some(t) = ctx.lookup_local(&name.text) {
                return t;
            }
            if let Some(b) = ctx.binding(&name.text) {
                match b.kind {
                    "uniform" => return b.wgsl_ty.clone(),
                    // Bare storage names are arrays — avoid treating them as scalars.
                    "storage" => return format!("array<{}>", b.wgsl_ty),
                    "texture" | "storage_texture" | "sampler" => return b.wgsl_ty.clone(),
                    _ => {}
                }
            }
            "i32".into()
        }
        ExpressionNode::FunctionCall(name, _, args) | ExpressionNode::MethodCall(_, name, _, args) => {
            if let Some(ty) = builtin_return_wgsl_ty(&name.text, args.len()) {
                return ty.into();
            }
            match name.text.as_str() {
                // Zero-arg `StructName()` constructor → WGSL struct type.
                other if args.is_empty() => escape_wgsl_ident(other),
                other => ctx
                    .helper_returns
                    .get(other)
                    .cloned()
                    .unwrap_or_else(|| "i32".into()),
            }
        }
        ExpressionNode::Call(callee, _, args) => match &**callee {
            ExpressionNode::Identifier(n) => {
                if let Some(ty) = builtin_return_wgsl_ty(&n.text, args.len()) {
                    return ty.into();
                }
                if args.is_empty() {
                    return escape_wgsl_ident(&n.text);
                }
                ctx.helper_returns
                    .get(&n.text)
                    .cloned()
                    .unwrap_or_else(|| "i32".into())
            }
            ExpressionNode::MemberAccess(_, method) => {
                if let Some(ty) = builtin_return_wgsl_ty(&method.text, args.len()) {
                    return ty.into();
                }
                "i32".into()
            }
            _ => "i32".into(),
        },
        _ => "i32".into(),
    }
}
