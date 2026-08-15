//! Call-expression analysis, grouped by call shape:
//! - [`free_functions`]: free-function calls and overload selection entry points.
//! - [`member_calls`]: instance/static/namespaced method calls, plus the indexer/enumerator "hook"
//!   resolution (`@get_indexer`/`@set_indexer`/`@iterator`/`@next`) used to desugar `obj[i]`, `obj[i] = v`, and
//!   `for..in`. `resolve_hook_or_diagnose` there is the shared entry point those desugaring sites
//!   call; the `HookResolution` outcome is an implementation detail kept private to that module.
//! - [`overload_resolution`]: scoring/ranking of candidate overloads.
//! - [`constructor`]: constructor-call analysis.
//! - [`args`]: argument typing, named-arg reorder, variadic packing, ref validation.

pub(crate) mod args;
pub(crate) mod constructor;
pub(crate) mod free_functions;
pub(crate) mod member_calls;
pub(crate) mod overload_resolution;

use super::*;
use dream_diagnostics::DiagnosticBag;
use dream_syntax::nodes::{ExpressionNode, FunctionNode, Type};
use dream_text::text_span::TextSpan;

impl<'a> Analyzer<'a> {
    /// True when `expr` is an `async (params) => …` lambda literal (possibly nested in parens).
    /// Used to pick between sync/`Future`-returning `fun(...)` overloads before arguments are typed.
    pub(super) fn is_async_lambda_expr(expr: &ExpressionNode<'_>) -> bool {
        match expr {
            ExpressionNode::Lambda(l) => l.is_async,
            ExpressionNode::Parenthesized(_, inner) => Self::is_async_lambda_expr(inner),
            _ => false,
        }
    }

    /// True when `ty` is `fun(...): Future<T>` for some `T`.
    pub(super) fn is_future_returning_fun(ty: &Type) -> bool {
        match ty {
            Type::Function(_, ret) => Self::future_inner_type(ret).is_some(),
            _ => false,
        }
    }

    /// Soft expected-param hint for an overloaded callee whose overloads differ by
    /// `fun(...): T` vs `fun(...): Future<T>`: pick the overload matching whether each argument is
    /// an async lambda. `skip` drops leading parameters (e.g. implicit `this` for instance methods).
    pub(super) fn expected_params_preferring_fun_overload(
        &self,
        base: &str,
        args: &[ExpressionNode<'_>],
        skip: usize,
    ) -> Option<Vec<Type>> {
        let keys = self.function_table.overloads.get(base)?;
        let mut matching: Vec<crate::function_table::FunctionTableInfo> = Vec::new();
        for key in keys {
            let Ok(info) = self.function_table.get_function(key) else {
                continue;
            };
            let user_params = info.parameters.len().saturating_sub(skip);
            if user_params != args.len() {
                continue;
            }
            matching.push(info);
        }
        if matching.is_empty() {
            return None;
        }
        if matching.len() == 1 {
            return Some(Self::expected_param_types(&matching[0])[skip..].to_vec());
        }
        let mut best_idx: Option<usize> = None;
        let mut best_score = -1i32;
        for (idx, info) in matching.iter().enumerate() {
            let types = Self::expected_param_types(info);
            let user = &types[skip..];
            let mut score = 0i32;
            for (i, arg) in args.iter().enumerate() {
                let Some(param_ty) = user.get(i) else {
                    continue;
                };
                let wants_future = Self::is_async_lambda_expr(arg);
                if Self::is_future_returning_fun(param_ty) == wants_future {
                    score += 1;
                }
            }
            if score > best_score {
                best_score = score;
                best_idx = Some(idx);
            } else if score == best_score {
                best_idx = None;
            }
        }
        let chosen = best_idx
            .and_then(|i| matching.get(i))
            .or_else(|| matching.first())?;
        Some(Self::expected_param_types(chosen)[skip..].to_vec())
    }

    /// Among same-arity `fun(...)`-typed parameter candidates, prefer the one whose parameter
    /// matches whether the corresponding argument is an async lambda (`Future`-returning fun) or
    /// not. Used when soft expected-type hints must be published before overload resolution.
    pub(super) fn prefer_fun_overload_for_args<'b, I>(
        candidates: I,
        args: &[ExpressionNode<'_>],
    ) -> Option<&'b FunctionNode<'b>>
    where
        I: IntoIterator<Item = &'b FunctionNode<'b>>,
    {
        let cands: Vec<&'b FunctionNode<'b>> = candidates.into_iter().collect();
        if cands.is_empty() {
            return None;
        }
        if cands.len() == 1 {
            return Some(cands[0]);
        }
        // Score each candidate by how many argument slots agree on async-lambda ↔ Future-fun.
        let mut best: Option<&'b FunctionNode<'b>> = None;
        let mut best_score = -1i32;
        for c in &cands {
            let mut score = 0i32;
            for (i, arg) in args.iter().enumerate() {
                let Some(param) = c.parameters.get(i) else {
                    continue;
                };
                let wants_future = Self::is_async_lambda_expr(arg);
                if Self::is_future_returning_fun(&param.type_) == wants_future {
                    score += 1;
                }
            }
            if score > best_score {
                best_score = score;
                best = Some(c);
            } else if score == best_score {
                best = None; // tie
            }
        }
        best.or_else(|| cands.first().copied())
    }

    /// The structured (never string-mangled) parameter types of `sig`, suitable for publishing as
    /// `current_expected_type` while analyzing a non-overloaded callee's arguments. Prefers
    /// `sig.parameter_types` (populated by `FunctionTableInfo::from`, so a generic-struct-typed
    /// parameter like `List<int>` keeps its `Struct(name, Some(args))` shape); falls back to
    /// reconstructing from the mangled `sig.parameters` strings for synthesized/stdlib entries
    /// that never populate `parameter_types` (only ever primitives there, so the round-trip is
    /// lossless).
    pub(super) fn expected_param_types(
        sig: &crate::function_table::FunctionTableInfo,
    ) -> Vec<Type> {
        if sig.parameter_types.len() == sig.parameters.len() {
            sig.parameter_types.clone()
        } else {
            sig.parameters
                .iter()
                .map(|p| Self::type_from_name(p))
                .collect()
        }
    }

    fn current_function_is_trusted_prelude(&self) -> bool {
        self.current_file
            .as_deref()
            .is_some_and(|f| f.starts_with("<std>/"))
    }

    /// Rejects a call to an `@unsafe` function/method/constructor unless the calling function is
    /// itself `@unsafe` (`current_function_is_unsafe`) or is part of the trusted stdlib prelude
    /// (`current_function_is_trusted_prelude`). Called at every direct-call resolution site (free
    /// function, static method, instance method) once the callee's `FunctionTableInfo` is resolved.
    pub(super) fn check_unsafe_call(
        &self,
        callee: &crate::function_table::FunctionTableInfo,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if callee.is_unsafe
            && !self.current_function_is_unsafe
            && !self.current_function_is_trusted_prelude()
        {
            diagnostics.report_error(
                format!(
                    "call to '@unsafe' function '{}' is only allowed from another '@unsafe' function or method",
                    callee.name
                ),
                Some(position),
            );
        }
    }

    /// Like [`Self::check_unsafe_call`], but for a `@intrinsic`-tagged template resolved inline
    /// (before a `FunctionTableInfo` exists for it), e.g. `Buffer.realloc`/`Buffer.free`. Checks the
    /// template's own attribute list directly rather than a looked-up callee.
    pub(super) fn check_unsafe_intrinsic_call(
        &self,
        name: &str,
        template: &dream_syntax::nodes::FunctionNode<'_>,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let is_unsafe = template.attributes.iter().any(|a| a.name.text == "unsafe");
        if is_unsafe
            && !self.current_function_is_unsafe
            && !self.current_function_is_trusted_prelude()
        {
            diagnostics.report_error(
                format!(
                    "call to '@unsafe' function '{}' is only allowed from another '@unsafe' function or method",
                    name
                ),
                Some(position),
            );
        }
    }

    /// Rejects a call when the callee is not available on the active compile target(s), unless the
    /// calling function is itself unavailable on those targets (so stdlib bodies that call other
    /// restricted APIs can still be analyzed when imported under a mismatched target).
    pub(super) fn check_runtime_call(
        &self,
        display_name: &str,
        runtime_support: dream_abi::attributes::RuntimeSupport,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if self.compile_targets.allows(runtime_support) {
            return;
        }
        if !self.compile_targets.allows(self.current_function_runtime) {
            return;
        }
        let target = self
            .compile_targets
            .first_missing_target(runtime_support)
            .unwrap_or("native");
        let target_label = if self.compile_targets.native as u8
            + self.compile_targets.node as u8
            + self.compile_targets.web as u8
            > 1
        {
            format!("compile targets '{}'", self.compile_targets.display_list())
        } else {
            format!("compile target '{target}'")
        };
        diagnostics.report_error(
            format!(
                "'{display_name}' is not available on {target_label} (available on: {})",
                runtime_support.display()
            ),
            Some(position),
        );
    }

    /// Like [`Self::check_runtime_call`], but for a template resolved inline before a
    /// `FunctionTableInfo` exists (e.g. `@js` externs on intrinsic paths).
    pub(super) fn check_runtime_intrinsic_call(
        &self,
        display_name: &str,
        template: &dream_syntax::nodes::FunctionNode<'_>,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let runtime_support =
            dream_abi::attributes::RuntimeSupport::from_attributes(&template.attributes);
        self.check_runtime_call(display_name, runtime_support, position, diagnostics);
    }

    /// Rejects calls that cross the CPU / GPU-shader boundary.
    ///
    /// - A `@compute` / `@vertex` / `@fragment` cannot be called like a CPU function.
    /// - From inside a shader, v1 only allows other GPU shaders and `Gpu` / `GpuMath` / `GpuVec*` helpers.
    pub(super) fn check_compute_call(
        &self,
        callee: &crate::function_table::FunctionTableInfo,
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        if callee.is_gpu_helper && !self.current_function_is_gpu {
            diagnostics.report_error(
                format!(
                    "cannot call @gpu helper '{}' from CPU code; it is only callable from @compute/@vertex/@fragment/@gpu functions",
                    callee.name
                ),
                Some(position),
            );
            return;
        }
        if callee.is_gpu_shader() && !self.current_function_is_gpu {
            let kind = if callee.is_compute {
                "@compute kernel"
            } else if callee.is_vertex {
                "@vertex shader"
            } else {
                "@fragment shader"
            };
            let hint = if callee.is_compute {
                format!(
                    "use Compute.run(\"{}\", ...) instead",
                    callee.name
                )
            } else {
                "link it via GpuRenderPipeline.create(...) instead".to_string()
            };
            diagnostics.report_error(
                format!(
                    "cannot call {kind} '{}' like a CPU function; {hint}",
                    callee.name
                ),
                Some(position),
            );
            return;
        }
        if self.current_function_is_gpu && !callee.is_gpu_shader() && !callee.is_gpu_helper {
            let allowed_helper = callee.name.starts_with("Gpu_")
                || callee.name.starts_with("GpuMath_")
                || callee.name.starts_with("GpuVec2_")
                || callee.name.starts_with("GpuVec3_")
                || callee.name.starts_with("GpuVec4_")
                || callee.name.starts_with("GpuMat2_")
                || callee.name.starts_with("GpuMat3_")
                || callee.name.starts_with("GpuMat4_")
                || callee.name == "Gpu_workgroup_barrier"
                || callee.name == "Gpu_storage_barrier";
            if !allowed_helper {
                diagnostics.report_error(
                    format!(
                        "cannot call non-GPU function '{}' from a GPU shader; mark helpers with @gpu, or call @compute/@vertex/@fragment / GpuMath builtins",
                        callee.name
                    ),
                    Some(position),
                );
            }
        }
    }

    /// When both names are string literals, validate `@vertex`/`@fragment` pairing and interface types.
    pub(super) fn check_render_pipeline_create(
        &self,
        args: &[ExpressionNode<'_>],
        position: TextSpan,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(vs_name) = string_literal_text(args.first()) else {
            return;
        };
        let Some(fs_name) = string_literal_text(args.get(1)) else {
            return;
        };
        let vs = match self.function_table.get_function(&vs_name) {
            Ok(info) if info.is_vertex => info,
            Ok(_) => {
                diagnostics.report_error(
                    format!("GpuRenderPipeline.create: '{}' is not a @vertex shader", vs_name),
                    Some(position),
                );
                return;
            }
            Err(_) => {
                diagnostics.report_error(
                    format!(
                        "GpuRenderPipeline.create: unknown vertex shader '{}'",
                        vs_name
                    ),
                    Some(position),
                );
                return;
            }
        };
        let fs = match self.function_table.get_function(&fs_name) {
            Ok(info) if info.is_fragment => info,
            Ok(_) => {
                diagnostics.report_error(
                    format!(
                        "GpuRenderPipeline.create: '{}' is not a @fragment shader",
                        fs_name
                    ),
                    Some(position),
                );
                return;
            }
            Err(_) => {
                diagnostics.report_error(
                    format!(
                        "GpuRenderPipeline.create: unknown fragment shader '{}'",
                        fs_name
                    ),
                    Some(position),
                );
                return;
            }
        };
        let vs_ret = match &vs.return_type {
            Some(Type::Struct(tok, None)) => tok.text.clone(),
            _ => String::new(),
        };
        let fs_in = fs
            .parameter_types
            .first()
            .and_then(|t| match t {
                Type::Struct(tok, None)
                    if !matches!(tok.text.as_str(), "GpuTexture" | "GpuSampler") =>
                {
                    Some(tok.text.clone())
                }
                _ => None,
            })
            .unwrap_or_default();
        if !vs_ret.is_empty() && !fs_in.is_empty() && vs_ret != fs_in {
            diagnostics.report_error(
                format!(
                    "GpuRenderPipeline.create: @vertex '{}' returns '{}' but @fragment '{}' expects '{}'",
                    vs_name, vs_ret, fs_name, fs_in
                ),
                Some(position),
            );
        }
    }
}

fn string_literal_text(expr: Option<&ExpressionNode<'_>>) -> Option<String> {
    match expr? {
        ExpressionNode::Literal(Type::String(tok)) => {
            let raw = tok.text.as_str();
            let inner = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(raw);
            Some(inner.to_string())
        }
        _ => None,
    }
}
