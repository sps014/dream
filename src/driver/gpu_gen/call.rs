//! WGSL lowering for shader builtin and helper calls.

use super::context::EmitCtx;
use super::expr::{coerce_expr_to_wgsl_ty, emit_expr};
use super::ident::escape_wgsl_ident;
use super::ty::{infer_wgsl_ty, is_mat_wgsl, is_vec_wgsl};
use dream_syntax::nodes::expression::ExpressionNode;

pub(super) fn emit_call(name: &str, args: &[ExpressionNode<'_>], ctx: &EmitCtx<'_>) -> String {
    let coerce_all = |want: &str| -> Vec<String> {
        args.iter()
            .map(|a| coerce_expr_to_wgsl_ty(a, want, ctx))
            .collect()
    };
    // Argument `i` coerced to `ty`. The fallback only matters for a call the analyzer already
    // rejected for its argument count, where emitting something type-correct keeps the WGSL
    // parseable so the real diagnostic is what the user sees.
    let at = |i: usize, ty: &str, fallback: &str| -> String {
        args.get(i)
            .map(|a| coerce_expr_to_wgsl_ty(a, ty, ctx))
            .unwrap_or_else(|| fallback.to_string())
    };
    // Argument `i` as-is, for resources (textures, samplers, buffers) that have no coercion.
    let raw = |i: usize, fallback: &str| -> String {
        args.get(i)
            .map(|a| emit_expr(a, ctx))
            .unwrap_or_else(|| fallback.to_string())
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
        "atomic_store" | "atomic_add" | "atomic_sub" | "atomic_min" | "atomic_max"
        | "atomic_and" | "atomic_or" | "atomic_xor" | "atomic_exchange" => {
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
                "atomic_sub" => "atomicSub",
                "atomic_min" => "atomicMin",
                "atomic_max" => "atomicMax",
                "atomic_and" => "atomicAnd",
                "atomic_or" => "atomicOr",
                "atomic_xor" => "atomicXor",
                _ => "atomicExchange",
            };
            format!("{op}(&{buf}[u32({idx})], {val})")
        }
        "atomic_compare_exchange" => format!(
            // WGSL returns a struct of the old value and whether the store happened. Dream
            // exposes the old value alone, which the caller compares against `cmp` — the standard
            // CAS-loop idiom, and the only part that has a Dream type today.
            "atomicCompareExchangeWeak(&{}[u32({})], {}, {}).old_value",
            raw(0, "buf"),
            at(1, "i32", "0"),
            at(2, "i32", "0"),
            at(3, "i32", "0")
        ),
        // The WGSL pack builtins yield a u32; Dream's `int` is signed, so the bit pattern is
        // reinterpreted rather than converted (a value conversion would saturate anything with
        // the top bit set).
        // These share their WGSL spelling, so the name passes through unchanged.
        "pack4x8unorm" | "pack2x16unorm" | "pack2x16snorm" => {
            format!("bitcast<i32>({name}({}))", raw(0, "v"))
        }
        "unpack4x8unorm" | "unpack4x8snorm" | "unpack2x16unorm" | "unpack2x16snorm" => {
            format!("{name}(bitcast<u32>({}))", at(0, "i32", "0"))
        }
        "count_one_bits" => {
            let val = args
                .first()
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            format!("i32(countOneBits(u32({val})))")
        }
        "reverse_bits" => {
            let val = args
                .first()
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            format!("i32(reverseBits(u32({val})))")
        }
        "count_leading_zeros" => {
            let val = args
                .first()
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            format!("i32(countLeadingZeros(u32({val})))")
        }
        "count_trailing_zeros" => {
            let val = args
                .first()
                .map(|a| coerce_expr_to_wgsl_ty(a, "i32", ctx))
                .unwrap_or_else(|| "0".into());
            format!("i32(countTrailingZeros(u32({val})))")
        }
        "texture_dimensions" => {
            let tex = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "tex".into());
            format!("vec2<f32>(textureDimensions({tex}))")
        }
        "texture_sample_cube" => {
            let tex = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "tex".into());
            let samp = args
                .get(1)
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "samp".into());
            let dir = args
                .get(2)
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "vec3<f32>(0.0, 0.0, 1.0)".into());
            format!("textureSample({tex}, {samp}, {dir})")
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
        "texture_load_level" => format!(
            "textureLoad({}, vec2<i32>({}, {}), {})",
            raw(0, "tex"),
            at(1, "i32", "0"),
            at(2, "i32", "0"),
            at(3, "i32", "0")
        ),
        "texture_load_layer" => format!(
            "textureLoad({}, vec2<i32>({}, {}), {}, {})",
            raw(0, "tex"),
            at(1, "i32", "0"),
            at(2, "i32", "0"),
            at(3, "i32", "0"),
            at(4, "i32", "0")
        ),
        // WGSL returns these as u32; Dream types texture metadata as `int`.
        "texture_num_levels" => format!("i32(textureNumLevels({}))", raw(0, "tex")),
        "texture_num_layers" => format!("i32(textureNumLayers({}))", raw(0, "tex")),
        "texture_sample_layer" => format!(
            "textureSample({}, {}, vec2<f32>({}, {}), {})",
            raw(0, "tex"),
            raw(1, "samp"),
            at(2, "f32", "0.0"),
            at(3, "f32", "0.0"),
            at(4, "i32", "0")
        ),
        "texture_sample_bias" => format!(
            "textureSampleBias({}, {}, vec2<f32>({}, {}), {})",
            raw(0, "tex"),
            raw(1, "samp"),
            at(2, "f32", "0.0"),
            at(3, "f32", "0.0"),
            at(4, "f32", "0.0")
        ),
        "texture_sample_grad" => format!(
            "textureSampleGrad({}, {}, {}, {}, {})",
            raw(0, "tex"),
            raw(1, "samp"),
            at(2, "vec2<f32>", "vec2<f32>(0.0)"),
            at(3, "vec2<f32>", "vec2<f32>(0.0)"),
            at(4, "vec2<f32>", "vec2<f32>(0.0)")
        ),
        "texture_gather" => {
            // WGSL wants the component as a const-expression, so a literal is the only form that
            // can be lowered; anything else would emit WGSL that fails to compile.
            let literal = match args.first() {
                Some(ExpressionNode::Literal(dream_syntax::nodes::Type::Integer(tok))) => {
                    tok.text.parse::<u32>().ok().filter(|&c| c < 4)
                }
                _ => None,
            };
            let component = match literal {
                Some(c) => c.to_string(),
                None => {
                    ctx.report_error(
                        format!(
                            "GPU shader '{}' needs a literal 0, 1, 2, or 3 for the component of \
                             Gpu.texture_gather",
                            ctx.kernel
                        ),
                        args.first().and_then(|a| a.position()),
                    );
                    "0".to_string()
                }
            };
            format!(
                "textureGather({component}, {}, {}, vec2<f32>({}, {}))",
                raw(1, "tex"),
                raw(2, "samp"),
                at(3, "f32", "0.0"),
                at(4, "f32", "0.0")
            )
        }
        "texture_sample_compare" => format!(
            "textureSampleCompare({}, {}, vec2<f32>({}, {}), {})",
            raw(0, "tex"),
            raw(1, "samp"),
            at(2, "f32", "0.0"),
            at(3, "f32", "0.0"),
            at(4, "f32", "0.0")
        ),
        "texture_sample_compare_level" => format!(
            "textureSampleCompareLevel({}, {}, vec2<f32>({}, {}), {})",
            raw(0, "tex"),
            raw(1, "samp"),
            at(2, "f32", "0.0"),
            at(3, "f32", "0.0"),
            at(4, "f32", "0.0")
        ),
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
        "inverse" => {
            let m = args
                .first()
                .map(|a| emit_expr(a, ctx))
                .unwrap_or_else(|| "m".into());
            let ty = args
                .first()
                .map(|a| infer_wgsl_ty(a, ctx))
                .unwrap_or_else(|| "mat2x2<f32>".into());
            // WGSL has no `inverse` builtin, so this calls a helper emitted alongside the entry
            // point (see `intrinsic_wgsl`), which mirrors `GpuMath.inverse` on the CPU.
            let helper = match ty.as_str() {
                "mat2x2<f32>" => "dream_inverse_mat2",
                "mat3x3<f32>" => "dream_inverse_mat3",
                _ => "dream_inverse_mat4",
            };
            format!("{helper}({m})")
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
        "min" | "max" | "abs" | "clamp" | "sqrt" | "floor" | "ceil" | "fract" | "sin" | "cos"
        | "tan" | "asin" | "acos" | "atan" | "atan2" | "normalize" | "length" | "dot" | "cross"
        | "reflect" | "refract" | "faceforward" | "distance" | "mix" | "pow" | "exp" | "exp2"
        | "log" | "log2" | "round" | "trunc" | "radians" | "degrees" | "sign" | "saturate"
        | "step" | "smoothstep" | "fma" | "inversesqrt" | "determinant" | "dpdx" | "dpdy"
        | "fwidth" => {
            let arg_tys: Vec<String> = args.iter().map(|a| infer_wgsl_ty(a, ctx)).collect();
            let any_vec = arg_tys.iter().any(|t| is_vec_wgsl(t));
            let any_mat = arg_tys.iter().any(|t| is_mat_wgsl(t));
            let vec_ty = arg_tys.iter().find(|t| is_vec_wgsl(t)).cloned();
            let args_s: Vec<String> = if any_mat {
                args.iter().map(|a| emit_expr(a, ctx)).collect()
            } else if any_vec {
                args.iter()
                    .zip(arg_tys.iter())
                    .map(|(a, ty)| {
                        let rendered = emit_expr(a, ctx);
                        if *ty == "f32" {
                            if let Some(vty) = vec_ty.as_deref() {
                                if name == "mix" || name == "refract" {
                                    // WGSL `mix(vec, vec, f32)` and `refract(vec, vec, f32)` keep a scalar factor.
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
                "faceforward" => format!(
                    "faceForward({}, {}, {})",
                    args_s.first().cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(1).cloned().unwrap_or_else(|| "0.0".into()),
                    args_s.get(2).cloned().unwrap_or_else(|| "0.0".into())
                ),
                "refract" => format!(
                    "refract({}, {}, {})",
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
                "distance" | "atan2" | "step" | "pow" => format!(
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
                "normalize" | "length" | "cross" | "reflect" | "dot" | "exp" | "exp2" | "log"
                | "log2" | "round" | "trunc" | "radians" | "degrees" | "sign" | "inversesqrt"
                | "determinant" | "dpdx" | "dpdy" | "fwidth" => {
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
