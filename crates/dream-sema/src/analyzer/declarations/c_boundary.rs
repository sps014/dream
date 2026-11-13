//! `@c` boundary classification: which Dream types cross into C and how ([`CShape`]). The same
//! walk validates a declaration (reporting each unsupported type) and feeds [`HImport`]'s shapes,
//! so the backend never re-derives the mapping from type names.
//!
//! [`HImport`]: dream_hir::HImport

use super::*;
use dream_abi::attributes::{OwnedResult, c_import_target, owned_result};
use dream_abi::c_abi::{
    C_PTR_TYPE, NATIVE_CALLBACK_TYPE, OWNED_C_PTR_TYPE, is_c_identifier, is_link_name,
};
use dream_hir::CShape;
use dream_syntax::nodes::{ConstraintKind, ExpressionNode, FunctionNode, ParameterNode, Type};
use dream_types::CScalar;

/// Where a type sits, which decides what it may be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pos {
    Param,
    Return,
    CallbackParam,
    CallbackReturn,
}

const PARAM_TYPES: &str = "numbers, `bool`, `string`, `CPtr`, @unmanaged structs, `fun(...)`, \
     `NativeCallback<fun(...)>`, `Option` of `string`/`CPtr`/`fun`/`NativeCallback`, and arrays of \
     numbers or @unmanaged structs";
const RETURN_TYPES: &str = "`void`, numbers, `bool`, `string`, `byte[]` and arrays of numbers or \
     @unmanaged structs, `CPtr`, @unmanaged structs, `OwnedCPtr` (with `@owned(\"free_fn\")`), \
     `@owned` `string`, `Option<string>`, and `Option<CPtr>`";
const CALLBACK_PARAM_TYPES: &str =
    "numbers, `bool`, `string`, `CPtr`, `Option<string>`, and `Option<CPtr>`";
const CALLBACK_RETURN_TYPES: &str = "`void`, numbers, `bool`, and `CPtr`";

impl<'a> Analyzer<'a> {
    /// Reports every parameter or result of a `@c` extern that cannot cross the C boundary.
    pub(in crate::analyzer) fn validate_c_extern_signature(
        &mut self,
        function: &FunctionNode<'a>,
        def: dream_types::DefId,
        diagnostics: &mut DiagnosticBag,
    ) {
        if function.generic_parameters.is_some() {
            diagnostics.report_error(
                format!(
                    "'@c' extern '{}' cannot be generic; C has no type parameters",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
        if function.parameters.iter().any(|p| p.is_variadic) {
            diagnostics.report_error(
                format!(
                    "'@c' extern '{}' cannot be variadic; declare one overload per arity",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
        let mut wrapped = Vec::new();
        for (i, param) in function.parameters.iter().enumerate() {
            match self.c_param_shape(param) {
                Ok(s) if s.needs_wrapper() => wrapped.push(i),
                Ok(_) => {}
                Err(msg) => diagnostics.report_error(
                    format!(
                        "'@c' extern '{}' parameter '{}': {msg}",
                        function.name.text, param.name.text
                    ),
                    Some(param.name.position),
                ),
            }
        }
        if !wrapped.is_empty() {
            self.c_wrapped_fun_params.insert(def, wrapped);
        }
        if let Err(msg) = self.c_ret_shape(function) {
            diagnostics.report_error(
                format!("'@c' extern '{}' result: {msg}", function.name.text),
                Some(function.name.position),
            );
        }
        let (lib, symbol) = c_import_target(&function.attributes)
            .unwrap_or_else(|| (String::new(), function.name.text.clone()));
        if !lib.is_empty() && !is_link_name(&lib) {
            diagnostics.report_error(
                format!(
                    "'@c' extern '{}' names library '{lib}', which is not a link name",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
        // The per-file last-error accessor keeps a private Dream name and binds the set's
        // generated `dream__…` shim symbol. Every other `dream__` binding is the shim function
        // itself, whose Dream name is that symbol.
        let generated_shim = symbol == function.name.text
            || (function.name.text.starts_with("__cpp_last_error_")
                && symbol.starts_with("dream__"));
        if !is_c_identifier(&symbol) || (symbol.starts_with("dream__") && !generated_shim) {
            diagnostics.report_error(
                format!(
                    "'@c' extern '{}' binds '{symbol}', which is not a C identifier",
                    function.name.text
                ),
                Some(function.name.position),
            );
        }
    }

    /// A `fun` argument whose C signature needs conversion gets a wrapper per target function, so
    /// the target must be statically known: a named function or a lambda literal.
    pub(in crate::analyzer) fn check_c_fun_args(
        &self,
        callee: dream_types::DefId,
        args: &[ExpressionNode<'a>],
        arg_offset: usize,
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(wrapped) = self.c_wrapped_fun_params.get(&callee) else {
            return;
        };
        for &i in wrapped {
            let Some(i) = i.checked_sub(arg_offset) else {
                continue;
            };
            let Some(arg) = args.get(i) else {
                continue;
            };
            let known = match arg {
                ExpressionNode::Lambda(_) => true,
                ExpressionNode::Identifier(tok) => {
                    symbol_table.borrow().get_symbol(tok).is_err()
                        && self.function_info(&tok.text).is_ok()
                }
                _ => false,
            };
            if !known {
                diagnostics.report_error(
                    format!(
                        "argument {} of '@c' extern '{}' must be a named function or a lambda \
                         literal: its C signature needs a wrapper generated per target; wrap a \
                         `fun` value in NativeCallback instead",
                        i + 1,
                        self.type_ctx.defs.name(callee)
                    ),
                    arg.position(),
                );
            }
        }
    }

    /// `(parameter shapes, result shape)` of a `@c` extern. Validation already rejected every
    /// unsupported position, and imports are built only after analysis reported no errors, so a
    /// failure here is a compiler bug rather than a user program.
    pub(in crate::analyzer) fn c_shapes(
        &self,
        function: &FunctionNode<'a>,
    ) -> (Vec<CShape>, CShape) {
        let params = function
            .parameters
            .iter()
            .map(|p| match self.c_param_shape(p) {
                Ok(shape) => shape,
                Err(msg) => crate::internal_error!(
                    "@c extern '{}' parameter was not validated: {msg}",
                    function.name.text
                ),
            })
            .collect();
        let ret = match self.c_ret_shape(function) {
            Ok(shape) => shape,
            Err(msg) => crate::internal_error!(
                "@c extern '{}' result was not validated: {msg}",
                function.name.text
            ),
        };
        (params, ret)
    }

    fn c_param_shape(&self, param: &ParameterNode) -> Result<CShape, String> {
        if param.is_borrow {
            if param.type_.is_unknown() {
                return Ok(POISON);
            }
            if matches!(param.type_, Type::String(_)) {
                return Ok(CShape::StrView);
            }
            return Err(
                "`borrow` at the C boundary applies only to `string`; the view is valid for the call"
                    .to_string(),
            );
        }
        if param.is_ref {
            return self.c_ref_shape(&param.type_);
        }
        let user_data_last = dream_abi::attributes::user_data_position(&param.attributes)
            == Some(dream_abi::attributes::UserDataPosition::Last);
        let has_user_data = dream_abi::attributes::user_data_position(&param.attributes).is_some();
        let shape = self.c_shape(&param.type_, Pos::Param)?;
        match shape {
            CShape::Callback {
                params,
                ret,
                optional,
                ..
            } => Ok(CShape::Callback {
                params,
                ret,
                optional,
                user_data_last,
            }),
            _ if has_user_data => {
                Err("`@user_data` applies only to a `NativeCallback` parameter".to_string())
            }
            other => Ok(other),
        }
    }

    fn c_ret_shape(&self, function: &FunctionNode<'a>) -> Result<CShape, String> {
        let owned = owned_result(&function.attributes);
        let ret = function.return_type.as_ref().filter(|t| **t != Type::Void);
        match (ret, owned) {
            (Some(t), OwnedResult::FreedBy(free)) if self.is_owned_c_ptr(t) => {
                if is_c_identifier(free) {
                    Ok(CShape::OwnedPtr {
                        free: free.to_string(),
                    })
                } else {
                    Err(format!(
                        "`@owned` names '{free}', which is not a C identifier"
                    ))
                }
            }
            (Some(t), _) if self.is_owned_c_ptr(t) => Err(format!(
                "returning `{OWNED_C_PTR_TYPE}` needs `@owned(\"free_fn\")` naming the C function \
                 that frees the pointer"
            )),
            (_, OwnedResult::FreedBy(_)) => Err(format!(
                "`@owned(\"free_fn\")` applies only to a `@c` extern returning `{OWNED_C_PTR_TYPE}`"
            )),
            (Some(Type::String(_)), OwnedResult::Owned) => Ok(CShape::OwnedStr),
            (_, OwnedResult::Owned) => Err(
                "bare `@owned` on a `@c` extern applies only to a `string` result; return \
                 `OwnedCPtr` with `@owned(\"free_fn\")` for a raw pointer"
                    .to_string(),
            ),
            (None, _) => Ok(CShape::Void),
            (Some(t), _) => self.c_shape(t, Pos::Return),
        }
    }

    /// `ref p: T` passes `&p`: `T` must be a scalar, `CPtr`, or an @unmanaged struct.
    fn c_ref_shape(&self, ty: &Type) -> Result<CShape, String> {
        let ok =
            CScalar::of_type(ty).is_some() || self.is_c_ptr(ty) || self.is_unmanaged_struct(ty);
        if ok {
            Ok(CShape::Ref)
        } else {
            Err(format!(
                "`ref` type '{}' has no C representation; use a number, `CPtr`, or an @unmanaged struct",
                ty.display_name()
            ))
        }
    }

    fn c_shape(&self, ty: &Type, pos: Pos) -> Result<CShape, String> {
        if ty.is_unknown() {
            return Ok(POISON);
        }
        let shape = self.c_shape_inner(ty, pos);
        let allowed = match (&shape, pos) {
            (Some(CShape::Scalar(_)), _) => true,
            (Some(CShape::Ptr { optional: false }), Pos::CallbackReturn) => true,
            (Some(CShape::Str { .. } | CShape::Ptr { .. }), p) => p != Pos::CallbackReturn,
            (Some(CShape::Struct), p) => matches!(p, Pos::Param | Pos::Return),
            (Some(CShape::Func { .. } | CShape::Callback { .. }), Pos::Param) => true,
            (Some(CShape::Array), Pos::Param | Pos::Return) => true,
            _ => false,
        };
        match shape {
            Some(CShape::Struct) if allowed => {
                if let Some(problem) = self.c_struct_layout_problem(ty) {
                    Err(format!(
                        "type '{}' cannot cross the C boundary: it contains {problem}",
                        ty.display_name()
                    ))
                } else {
                    Ok(CShape::Struct)
                }
            }
            Some(s) if allowed => Ok(s),
            _ => Err(format!(
                "type '{}' has no C representation here; allowed: {}",
                ty.display_name(),
                match pos {
                    Pos::Param => PARAM_TYPES,
                    Pos::Return => RETURN_TYPES,
                    Pos::CallbackParam => CALLBACK_PARAM_TYPES,
                    Pos::CallbackReturn => CALLBACK_RETURN_TYPES,
                }
            )),
        }
    }

    fn c_shape_inner(&self, ty: &Type, pos: Pos) -> Option<CShape> {
        if let Some(s) = CScalar::of_type(ty) {
            return Some(CShape::Scalar(s));
        }
        if self.is_unmanaged_struct(ty) {
            return Some(CShape::Struct);
        }
        match ty {
            Type::String(_) => Some(CShape::Str { optional: false }),
            Type::Array(elem) if matches!(pos, Pos::Param | Pos::Return) => {
                let ok = is_c_array_elem(elem) || self.is_unmanaged_struct(elem);
                ok.then_some(CShape::Array)
            }
            Type::Function(params, ret) if pos == Pos::Param => {
                let (params, ret) = self.c_callback_sig(params, ret)?;
                Some(CShape::Func {
                    params,
                    ret: Box::new(ret),
                    optional: false,
                })
            }
            Type::Struct(tok, Some(args)) if tok.text == "Option" && args.len() == 1 => {
                let inner = self.c_shape_inner(&args[0], pos)?;
                optional(inner)
            }
            Type::Struct(tok, Some(args))
                if tok.text == NATIVE_CALLBACK_TYPE && args.len() == 1 && pos == Pos::Param =>
            {
                let Type::Function(params, ret) = &args[0] else {
                    return None;
                };
                let (params, ret) = self.c_callback_sig(params, ret)?;
                Some(CShape::Callback {
                    params,
                    ret: Box::new(ret),
                    optional: false,
                    user_data_last: false,
                })
            }
            _ if self.is_c_ptr(ty) => Some(CShape::Ptr { optional: false }),
            _ => None,
        }
    }

    fn c_callback_sig(&self, params: &[Type], ret: &Type) -> Option<(Vec<CShape>, CShape)> {
        let params = params
            .iter()
            .map(|p| self.c_shape(p, Pos::CallbackParam).ok())
            .collect::<Option<Vec<_>>>()?;
        let ret = match ret {
            Type::Void => CShape::Void,
            t => self.c_shape(t, Pos::CallbackReturn).ok()?,
        };
        Some((params, ret))
    }

    fn is_c_ptr(&self, ty: &Type) -> bool {
        matches!(ty, Type::Struct(tok, None) if tok.text == C_PTR_TYPE)
            && self
                .type_ctx
                .resolved_type(dream_abi::core_items::C_PTR)
                .is_some_and(|ty| self.struct_info(ty).is_some())
    }

    fn is_owned_c_ptr(&self, ty: &Type) -> bool {
        matches!(ty, Type::Struct(tok, None) if tok.text == OWNED_C_PTR_TYPE)
            && self
                .type_ctx
                .resolved_type(dream_abi::core_items::OWNED_C_PTR)
                .is_some_and(|ty| self.struct_info(ty).is_some())
    }

    /// `float16`, tuples, and enums have no C layout the shim can emit. Walk nested unmanaged
    /// structs; a cycle is impossible for a well-formed value struct, and a repeated name stops.
    fn c_struct_layout_problem(&self, ty: &Type) -> Option<String> {
        fn walk(this: &Analyzer<'_>, ty: &Type, seen: &mut Vec<String>) -> Option<String> {
            match ty {
                Type::Float16(_) => Some("`float16`".to_string()),
                Type::Tuple(_) => Some("a tuple".to_string()),
                Type::Struct(tok, _) => {
                    let name = tok.text.clone();
                    if this
                        .type_ctx
                        .resolve(dream_types::DefKind::Enum, &name)
                        .is_some()
                    {
                        return Some(format!("enum '{name}'"));
                    }
                    let id = this.type_ctx.resolved_type(&name)?;
                    if this.union_info(id).is_some() {
                        return Some(format!("union '{name}'"));
                    }
                    if seen.iter().any(|seen| seen == &name) {
                        return None;
                    }
                    let info = this.struct_info(id)?;
                    seen.push(name);
                    for (field, field_info) in &info.fields {
                        if let Some(problem) = walk(this, &field_info.type_, seen) {
                            return Some(format!("field '{field}' ({problem})"));
                        }
                    }
                    None
                }
                _ => None,
            }
        }
        walk(self, ty, &mut Vec::new())
    }

    fn is_unmanaged_struct(&self, ty: &Type) -> bool {
        let Type::Struct(tok, None) = ty else {
            return false;
        };
        tok.text != C_PTR_TYPE
            && self.type_ctx.resolved_type(&tok.text).is_some_and(|ty| {
                self.struct_info(ty).is_some()
                    && self.type_id_satisfies_kind(
                        ty,
                        ConstraintKind::Unmanaged,
                        &mut indexmap::IndexSet::new(),
                    )
            })
    }
}

fn optional(inner: CShape) -> Option<CShape> {
    match inner {
        CShape::Str { .. } => Some(CShape::Str { optional: true }),
        CShape::Ptr { .. } => Some(CShape::Ptr { optional: true }),
        CShape::Func { params, ret, .. } => Some(CShape::Func {
            params,
            ret,
            optional: true,
        }),
        CShape::Callback {
            params,
            ret,
            user_data_last,
            ..
        } => Some(CShape::Callback {
            params,
            ret,
            optional: true,
            user_data_last,
        }),
        _ => None,
    }
}

/// Unknown types (from an earlier error) keep a scalar shape so one mistake does not cascade.
const POISON: CShape = CShape::Scalar(CScalar::I32);

/// Element types whose Dream array storage is the C array (`bool`/`char` use Dream-specific widths).
fn is_c_array_elem(ty: &Type) -> bool {
    CScalar::of_type(ty).is_some_and(|s| !matches!(s, CScalar::Bool | CScalar::Char))
}
