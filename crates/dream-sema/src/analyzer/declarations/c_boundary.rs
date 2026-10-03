//! `@c` boundary classification: which Dream types cross into C and how ([`CShape`]). The same
//! walk validates a declaration (reporting each unsupported type) and feeds [`HImport`]'s shapes,
//! so the backend never re-derives the mapping from type names.
//!
//! [`HImport`]: dream_hir::HImport

use super::*;
use dream_abi::attributes::{c_import_target, owned_result, OwnedResult};
use dream_abi::c_abi::{
    is_c_identifier, C_PTR_TYPE, MARSHAL_USER_DATA_LAST, NATIVE_CALLBACK_TYPE, OWNED_C_PTR_TYPE,
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
const RETURN_TYPES: &str = "`void`, numbers, `bool`, `string`, `CPtr`, @unmanaged structs, \
     `OwnedCPtr` (with `@owned(\"free_fn\")`), `Option<string>`, and `Option<CPtr>`";
const CALLBACK_PARAM_TYPES: &str =
    "numbers, `bool`, `string`, `CPtr`, `Option<string>`, and `Option<CPtr>`";
const CALLBACK_RETURN_TYPES: &str = "`void`, numbers, `bool`, and `CPtr`";

impl<'a> Analyzer<'a> {
    /// Reports every parameter or result of a `@c` extern that cannot cross the C boundary.
    pub(in crate::analyzer) fn validate_c_extern_signature(
        &mut self,
        function: &FunctionNode<'a>,
        registered_name: &str,
        diagnostics: &mut DiagnosticBag,
    ) {
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
            self.c_wrapped_fun_params
                .insert(registered_name.to_string(), wrapped);
        }
        if let Err(msg) = self.c_ret_shape(function) {
            diagnostics.report_error(
                format!("'@c' extern '{}' result: {msg}", function.name.text),
                Some(function.name.position),
            );
        }
        let symbol = c_import_target(&function.attributes)
            .map(|(_, symbol)| symbol)
            .unwrap_or_else(|| function.name.text.clone());
        if !is_c_identifier(&symbol) {
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
        callee: &str,
        args: &[ExpressionNode<'a>],
        symbol_table: &Rc<RefCell<SymbolTable>>,
        diagnostics: &mut DiagnosticBag,
    ) {
        let Some(wrapped) = self.c_wrapped_fun_params.get(callee) else {
            return;
        };
        for &i in wrapped {
            let Some(arg) = args.get(i) else {
                continue;
            };
            let known = match arg {
                ExpressionNode::Lambda(_) => true,
                ExpressionNode::Identifier(tok) => {
                    symbol_table.borrow().get_symbol(tok).is_err()
                        && self.function_table.get_function(&tok.text).is_ok()
                }
                _ => false,
            };
            if !known {
                diagnostics.report_error(
                    format!(
                        "argument {} of '@c' extern '{callee}' must be a named function or a lambda \
                         literal: its C signature needs a wrapper generated per target; wrap a \
                         `fun` value in NativeCallback instead",
                        i + 1
                    ),
                    arg.position(),
                );
            }
        }
    }

    /// `(parameter shapes, result shape)` of a validated `@c` extern; invalid positions (already
    /// reported) fall back to an `int` scalar.
    pub(in crate::analyzer) fn c_shapes(
        &self,
        function: &FunctionNode<'a>,
    ) -> (Vec<CShape>, CShape) {
        let params = function
            .parameters
            .iter()
            .map(|p| self.c_param_shape(p).unwrap_or(POISON))
            .collect();
        let ret = self.c_ret_shape(function).unwrap_or(POISON);
        (params, ret)
    }

    fn c_param_shape(&self, param: &ParameterNode) -> Result<CShape, String> {
        if param.is_ref {
            return self.c_ref_shape(&param.type_);
        }
        let user_data_last = dream_abi::attributes::c_marshal_charset(&param.attributes)
            == Some(MARSHAL_USER_DATA_LAST);
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
            _ if user_data_last => Err(format!(
                "`@marshal(\"{MARSHAL_USER_DATA_LAST}\")` applies only to a `NativeCallback` parameter"
            )),
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
                ty.get_type()
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
            (Some(CShape::Func { .. } | CShape::Callback { .. } | CShape::Array), Pos::Param) => {
                true
            }
            _ => false,
        };
        match shape {
            Some(s) if allowed => Ok(s),
            _ => Err(format!(
                "type '{}' has no C representation here; allowed: {}",
                ty.get_type(),
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
            Type::Array(elem) if pos == Pos::Param => {
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
            && self.struct_table.get_struct(C_PTR_TYPE).is_some()
    }

    fn is_owned_c_ptr(&self, ty: &Type) -> bool {
        matches!(ty, Type::Struct(tok, None) if tok.text == OWNED_C_PTR_TYPE)
            && self.struct_table.get_struct(OWNED_C_PTR_TYPE).is_some()
    }

    fn is_unmanaged_struct(&self, ty: &Type) -> bool {
        let Type::Struct(tok, None) = ty else {
            return false;
        };
        tok.text != C_PTR_TYPE
            && self.struct_table.get_struct(&tok.text).is_some()
            && self.type_satisfies_kind(ty, ConstraintKind::Unmanaged)
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

/// The shape recorded for a position whose error was already reported.
const POISON: CShape = CShape::Scalar(CScalar::I32);

/// Element types whose Dream array storage is the C array (`bool`/`char` use Dream-specific widths).
fn is_c_array_elem(ty: &Type) -> bool {
    CScalar::of_type(ty).is_some_and(|s| !matches!(s, CScalar::Bool | CScalar::Char))
}
