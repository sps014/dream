//! Names shared by sema and MIR for the native C/C++ boundary, and the generated C shim every
//! `@c` call goes through ([`shim`]).

pub mod shim;

/// The stdlib opaque pointer struct (`system.CPtr`): one `usize` field, passed to C by value.
pub const C_PTR_TYPE: &str = "CPtr";

/// The stdlib class (`system.OwnedCPtr`) a `@c` extern with `@owned("free_fn")` returns: fields
/// `ptr: CPtr` then `free: usize` (the C function pointer its destructor calls).
pub const OWNED_C_PTR_TYPE: &str = "OwnedCPtr";

/// C keywords (C11 through C23) that can never name a C symbol.
const C_KEYWORDS: &[&str] = &[
    "alignas",
    "alignof",
    "auto",
    "bool",
    "break",
    "case",
    "char",
    "const",
    "constexpr",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "false",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "nullptr",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_assert",
    "struct",
    "switch",
    "thread_local",
    "true",
    "typedef",
    "typeof",
    "typeof_unqual",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
];

/// True when `name` can be spelled as-is in generated C: `[A-Za-z_][A-Za-z0-9_]*`, not a keyword,
/// and outside the prefix the generated shim reserves for its own functions.
pub fn is_c_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !C_KEYWORDS.contains(&name)
        && !name.starts_with(shim::SHIM_PREFIX)
}

/// The embedding API declared by the public `include/dream_embed.h`; every native program keeps
/// these exported so C linked into it can call them.
pub const EMBED_EXPORTS: [&str; 6] = [
    "dream_thread_attach",
    "dream_thread_detach",
    "dream_retain",
    "dream_release",
    "dream_set_panic_hook",
    "dream_set_platform",
];

/// The stdlib callback class (`system.NativeCallback<F>`), passed to C as `(fn, void* user_data)`.
pub const NATIVE_CALLBACK_TYPE: &str = "NativeCallback";

/// `@marshal` value putting a `NativeCallback`'s `user_data` after the callback's own parameters.
pub const MARSHAL_USER_DATA_LAST: &str = "user_data_last";

/// The generated C++ shim's `extern "C"` symbol for one `@cpp` member:
/// `dream__<set>__<Class>__<member>`, with `class` empty for free functions. `overload` is the
/// member's index among same-named members (0 keeps the bare name), since C symbols cannot
/// overload. Set, class, and member names are Dream identifiers, so the result is a valid C name.
pub fn cpp_shim_symbol(set: &str, class: &str, member: &str, overload: usize) -> String {
    let set = set.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
    if overload == 0 {
        format!("dream__{set}__{class}__{member}")
    } else {
        format!("dream__{set}__{class}__{member}_{overload}")
    }
}

#[cfg(test)]
mod tests {
    use super::{cpp_shim_symbol, is_c_identifier};

    #[test]
    fn c_identifiers_exclude_keywords_and_shim_names() {
        assert!(is_c_identifier("sqlite3_open"));
        assert!(is_c_identifier("_aligned_malloc"));
        assert!(is_c_identifier(&cpp_shim_symbol("m", "C", "f", 0)));
        assert!(!is_c_identifier("struct"));
        assert!(!is_c_identifier("1abc"));
        assert!(!is_c_identifier("foo@8"));
        assert!(!is_c_identifier("dream_cs_x"));
        assert!(!is_c_identifier(""));
    }

    #[test]
    fn shim_symbols_are_c_identifiers() {
        assert_eq!(
            cpp_shim_symbol("kvstore", "Store", "get", 0),
            "dream__kvstore__Store__get"
        );
        assert_eq!(
            cpp_shim_symbol("kv-store", "Store", "put", 2),
            "dream__kv_store__Store__put_2"
        );
        assert_eq!(
            cpp_shim_symbol("m", "", "version", 0),
            "dream__m____version"
        );
    }
}
