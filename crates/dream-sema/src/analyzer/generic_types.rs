use super::*;

/// Builds the generic substitution bindings (parameter name -> concrete type name) by
/// zipping declared generic parameters with the supplied concrete arguments. Extra
/// parameters or arguments beyond the common length are ignored (arity is validated
/// separately so a clear diagnostic is produced).
pub fn generic_bindings(params: &[SyntaxToken], args: &[Type]) -> GenericBindings {
    params
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (param.text.clone(), arg.clone()))
        .collect()
}

/// Looks up the concrete type bound to a generic parameter name, if any.
pub(super) fn lookup_binding(bindings: &GenericBindings, name: &str) -> Option<Type> {
    bindings.get(name).cloned()
}

/// Builds a mangled function name by appending each concrete type from the bindings in order,
/// e.g. base `swap` with bindings `[(T,int),(V,string)]` becomes `swap_int_string`. The mangled
/// spelling is a WASM-symbol concern, so the concrete `Type`s are stringified only here.
pub(super) fn mangle_bindings(base: &str, bindings: &GenericBindings) -> String {
    mangle_with_suffixes(base, bindings.values().map(|concrete| concrete.get_type()))
}

/// Rewrites a field type token that refers to a generic parameter (e.g. `T`, `T[]`)
/// into its concrete form, preserving the array suffix. Tokens that do not name a
/// generic parameter are returned unchanged.
pub(super) fn substitute_generic_token(
    token: &SyntaxToken,
    bindings: &GenericBindings,
) -> SyntaxToken {
    let mut result = token.clone();
    let (base, suffix) = if let Some(base) = token.text.strip_suffix("[]") {
        (base, "[]")
    } else {
        (token.text.as_str(), "")
    };
    if let Some(concrete) = lookup_binding(bindings, base) {
        result.text = format!("{}{}", concrete.get_type(), suffix);
    }
    result
}

/// Rewrites a structured field type, substituting any generic parameter that appears in it with
/// its bound concrete type. Unlike `substitute_generic_token` (which only understands `T`, `T[]`
/// on a flat token), this recurses through arrays, generic arguments, and function types, so a
/// field like `List<T>` becomes `List<JsonValue>` rather than being flattened.
pub fn substitute_generic_type(ty: &Type, bindings: &GenericBindings) -> Type {
    match ty {
        Type::Array(inner) => Type::Array(Box::new(substitute_generic_type(inner, bindings))),
        Type::Tuple(elems) => Type::Tuple(
            elems
                .iter()
                .map(|e| substitute_generic_type(e, bindings))
                .collect(),
        ),
        Type::Function(params, ret) => Type::Function(
            params
                .iter()
                .map(|p| substitute_generic_type(p, bindings))
                .collect(),
            Box::new(substitute_generic_type(ret, bindings)),
        ),
        Type::Generic(name) => lookup_binding(bindings, name).unwrap_or_else(|| ty.clone()),
        Type::Struct(token, args) => {
            // A bare struct whose name is itself a generic parameter (the common `T` case, since
            // unknown identifiers parse as `Type::Struct`).
            if args.is_none() {
                if let Some(concrete) = lookup_binding(bindings, &token.text) {
                    return concrete;
                }
            }
            let new_args = args.as_ref().map(|a| {
                a.iter()
                    .map(|x| substitute_generic_type(x, bindings))
                    .collect()
            });
            Type::Struct(token.clone(), new_args)
        }
        other => other.clone(),
    }
}

/// Extracts the declared generic parameter names (`["T", "V"]`) from an optional parameter-token
/// list, for registering a nominal def's arity in the [`TypeCtx`].
pub(super) fn generic_param_names(params: &Option<Vec<SyntaxToken>>) -> Vec<String> {
    params
        .as_deref()
        .map(|ps| ps.iter().map(|p| p.text.clone()).collect())
        .unwrap_or_default()
}
