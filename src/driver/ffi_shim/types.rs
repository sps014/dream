//! Which Dream types cross the `@cpp` boundary, and how each one is spelled on the three sides of
//! the bridge: the Dream wrapper, the `@c` extern it calls, and the C++ shim.

use std::collections::BTreeMap;

use dream_abi::c_abi::{C_PTR_TYPE, NATIVE_CALLBACK_TYPE};
use dream_syntax::nodes::Type;
use dream_types::CScalar;

/// The scalar a `@cpp` signature names. Pointer-sized integers stay out: a `@cpp` struct's layout
/// is checked against the Dream declaration before the target's pointer width is known.
pub(super) fn scalar_of(ty: &Type) -> Option<CScalar> {
    CScalar::of_type(ty).filter(|s| !matches!(s, CScalar::ISize | CScalar::USize))
}

/// Whether a `ref` of this type binds to the same C++ lvalue type Dream stores.
fn ref_ok(s: CScalar) -> bool {
    !s.is_narrow()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Elem {
    Scalar(CScalar),
    Struct(String),
}

/// A value that crosses the boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Bridge {
    Scalar(CScalar),
    Str {
        optional: bool,
    },
    Ptr {
        optional: bool,
    },
    /// Another `@cpp` class, passed as its handle.
    Class {
        name: String,
        optional: bool,
    },
    /// A `@cpp` value struct, passed by address.
    Struct(String),
    Array(Elem),
    /// A `fun(...)`, handed to C++ as a `std::function`-compatible callable.
    Fun {
        params: Vec<Bridge>,
        ret: Option<Box<Bridge>>,
        dream: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Ret {
    Void,
    Value(Bridge),
    /// `Result<T, string>`: a thrown exception becomes `Err(e.what())`.
    Result(Bridge),
}

#[derive(Debug, Clone)]
pub(super) struct Param {
    pub name: String,
    pub bridge: Bridge,
    pub is_ref: bool,
    /// The parameter's Dream type as written, for the wrapper signature.
    pub dream: String,
}

/// The `@cpp` classes and structs of the whole program, by Dream name, with their C++ names.
#[derive(Debug, Default)]
pub(super) struct Known {
    pub classes: BTreeMap<String, String>,
    pub structs: BTreeMap<String, String>,
}

const BRIDGEABLE: &str = "numbers, `bool`, `string`, `CPtr`, `@cpp` classes and structs, arrays \
                          of numbers or `@cpp` structs, `fun(...)`, and `Option` of `string`, \
                          `CPtr`, or a `@cpp` class";

impl Known {
    pub(super) fn param(&self, ty: &Type, is_ref: bool) -> Result<Bridge, String> {
        let b = self.value(ty)?;
        if is_ref {
            let ok = match &b {
                Bridge::Scalar(s) => ref_ok(*s),
                Bridge::Struct(_) => true,
                _ => false,
            };
            if !ok {
                return Err(format!(
                    "`ref` type '{}' has no C++ lvalue form; use `int`, `uint`, `long`, `ulong`, \
                     `float`, `double`, or a `@cpp` struct",
                    ty.display_name()
                ));
            }
        }
        Ok(b)
    }

    pub(super) fn ret(&self, ty: Option<&Type>) -> Result<Ret, String> {
        let Some(ty) = ty.filter(|t| !matches!(t, Type::Void)) else {
            return Ok(Ret::Void);
        };
        if let Some([ok, err]) = generic_args(ty, "Result") {
            if !matches!(err, Type::String(_)) {
                return Err(format!(
                    "a `@cpp` result must be `Result<T, string>`, not '{}'",
                    ty.display_name()
                ));
            }
            return Ok(Ret::Result(self.returned(ok)?));
        }
        Ok(Ret::Value(self.returned(ty)?))
    }

    fn returned(&self, ty: &Type) -> Result<Bridge, String> {
        let b = self.value(ty)?;
        match b {
            Bridge::Struct(_) | Bridge::Array(_) | Bridge::Fun { .. } => Err(format!(
                "a `@cpp` member cannot return '{}'; return a `@cpp` class, or fill a `ref` \
                 parameter",
                ty.display_name()
            )),
            b => Ok(b),
        }
    }

    fn value(&self, ty: &Type) -> Result<Bridge, String> {
        if let Some(s) = scalar_of(ty) {
            return Ok(Bridge::Scalar(s));
        }
        match ty {
            Type::String(_) => return Ok(Bridge::Str { optional: false }),
            Type::Array(inner) => {
                return match scalar_of(inner) {
                    Some(s) if !matches!(s, CScalar::Bool | CScalar::Char) => {
                        Ok(Bridge::Array(Elem::Scalar(s)))
                    }
                    _ => match self.struct_name(inner) {
                        Some(n) => Ok(Bridge::Array(Elem::Struct(n))),
                        None => Err(format!(
                            "array element type '{}' has no C++ representation; use numbers or \
                             a `@cpp` struct",
                            inner.display_name()
                        )),
                    },
                };
            }
            Type::Function(params, ret) => return self.fun(ty, params, ret),
            _ => {}
        }
        if let Some([inner]) = generic_args(ty, "Option") {
            return match self.value(inner) {
                Ok(Bridge::Str { .. }) => Ok(Bridge::Str { optional: true }),
                Ok(Bridge::Ptr { .. }) => Ok(Bridge::Ptr { optional: true }),
                Ok(Bridge::Class { name, .. }) => Ok(Bridge::Class {
                    name,
                    optional: true,
                }),
                _ => Err(format!(
                    "'{}' has no C++ representation; `Option` crosses the boundary only around \
                     `string`, `CPtr`, or a `@cpp` class",
                    ty.display_name()
                )),
            };
        }
        if let Type::Struct(tok, None) = ty {
            if tok.text == C_PTR_TYPE {
                return Ok(Bridge::Ptr { optional: false });
            }
            if self.classes.contains_key(&tok.text) {
                return Ok(Bridge::Class {
                    name: tok.text.clone(),
                    optional: false,
                });
            }
            if self.structs.contains_key(&tok.text) {
                return Ok(Bridge::Struct(tok.text.clone()));
            }
        }
        if let Type::Struct(tok, _) = ty
            && tok.text == NATIVE_CALLBACK_TYPE
        {
            return Err(
                "a `@cpp` member takes `fun(...)` directly; `NativeCallback` is for `@c`"
                    .to_string(),
            );
        }
        Err(format!(
            "type '{}' cannot cross the C++ boundary; allowed: {BRIDGEABLE}",
            ty.display_name()
        ))
    }

    fn fun(&self, ty: &Type, params: &[Type], ret: &Type) -> Result<Bridge, String> {
        let mut ps = Vec::with_capacity(params.len());
        for p in params {
            match self.value(p)? {
                b @ (Bridge::Scalar(_) | Bridge::Str { .. } | Bridge::Ptr { .. }) => ps.push(b),
                _ => {
                    return Err(format!(
                        "callback parameter type '{}' cannot cross the C++ boundary; use numbers, \
                         `string`, `CPtr`, or their `Option`",
                        p.display_name()
                    ));
                }
            }
        }
        let ret = match ret {
            Type::Void => None,
            r => match self.value(r)? {
                b @ (Bridge::Scalar(_) | Bridge::Ptr { optional: false }) => Some(Box::new(b)),
                _ => {
                    return Err(format!(
                        "callback result type '{}' cannot cross the C++ boundary; use `void`, a \
                         number, or `CPtr`",
                        r.display_name()
                    ));
                }
            },
        };
        Ok(Bridge::Fun {
            params: ps,
            ret,
            dream: dream_type(ty),
        })
    }

    fn struct_name(&self, ty: &Type) -> Option<String> {
        match ty {
            Type::Struct(tok, None) if self.structs.contains_key(&tok.text) => {
                Some(tok.text.clone())
            }
            _ => None,
        }
    }
}

fn generic_args<'t>(ty: &'t Type, name: &str) -> Option<&'t [Type]> {
    match ty {
        Type::Struct(tok, Some(args)) if tok.text == name => Some(args),
        _ => None,
    }
}

/// Source spelling of a type, re-parseable by the Dream parser.
pub(super) fn dream_type(ty: &Type) -> String {
    match ty {
        Type::Array(inner) => format!("{}[]", dream_type(inner)),
        Type::Struct(tok, Some(args)) => format!(
            "{}<{}>",
            tok.text,
            args.iter().map(dream_type).collect::<Vec<_>>().join(", ")
        ),
        Type::Function(params, ret) => format!(
            "fun({}): {}",
            params.iter().map(dream_type).collect::<Vec<_>>().join(", "),
            dream_type(ret)
        ),
        Type::Tuple(elems) => format!(
            "({})",
            elems.iter().map(dream_type).collect::<Vec<_>>().join(", ")
        ),
        other => other.display_name(),
    }
}

impl Bridge {
    /// The C type of a callback parameter/result as Dream's reverse trampoline passes it.
    pub(super) fn callback_c(&self) -> &'static str {
        match self {
            Bridge::Scalar(s) => s.c_name(),
            Bridge::Str { .. } => "const char*",
            _ => "void*",
        }
    }
}
