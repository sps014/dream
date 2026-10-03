//! Backend naming helpers.
//!
//! These derive the internal symbol names the semantic analyzer and codegen agree on (method,
//! constructor, and `@json` converter names). Memory layout belongs to the HIR layout table.

use dream_syntax::nodes::types::CONSTRUCTOR_NAME;

/// The internal name under which a struct method is registered in the function table and emitted in
/// codegen: the struct name and method name joined with `_` (e.g. `User_greet`). Array targets
/// (`int[]`) replace `[]` with `__arr` so the symbol is a valid WAT identifier (`int__arr_size`).
/// Single source of truth for method-name mangling; the derived-method helpers below build on it.
pub fn method_fn(struct_name: &str, method_name: &str) -> String {
    let safe_struct = struct_name.replace("[]", "__arr");
    format!("{}_{}", safe_struct, method_name)
}

/// The internal name under which a struct's user-defined constructor is registered/emitted
/// (e.g. `User_constructor`). Single source of truth for the constructor naming convention.
pub fn constructor_fn(struct_name: &str) -> String {
    method_fn(struct_name, CONSTRUCTOR_NAME)
}

/// The name of the compiler-derived `to_json` converter for a `@json` struct (e.g. `User_to_json`).
/// Single source of truth for the implicit naming contract shared by the `@json` source generator,
/// the type checker, and the codegen backend.
pub fn json_to_json_fn(struct_name: &str) -> String {
    method_fn(struct_name, "to_json")
}

/// The name of the compiler-derived `from_json` converter for a `@json` struct (e.g.
/// `User_from_json`). See [`json_to_json_fn`].
pub fn json_from_json_fn(struct_name: &str) -> String {
    method_fn(struct_name, "from_json")
}
