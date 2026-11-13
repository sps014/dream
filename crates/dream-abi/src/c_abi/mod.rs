//! Names shared by sema and MIR for the native C/C++ boundary, and the generated C shim every
//! `@c` call goes through ([`shim`]).

pub mod shim;

/// The stdlib opaque pointer struct (`system.CPtr`): one `usize` field, passed to C by value.
pub use crate::core_items::C_PTR_NAME as C_PTR_TYPE;

/// The stdlib class (`system.OwnedCPtr`) a `@c` extern with `@owned("free_fn")` returns: fields
/// `ptr: CPtr` then `free: usize` (the C function pointer its destructor calls).
pub use crate::core_items::OWNED_C_PTR_NAME as OWNED_C_PTR_TYPE;

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

/// C and extension spellings a user symbol must not take, on top of [`C_KEYWORDS`].
const RESERVED_C_NAMES: &[&str] = &["_Atomic", "_Bool", "_Generic", "__attribute__", "asm"];

/// True when `name` can be spelled as-is in generated C: `[A-Za-z_][A-Za-z0-9_]*`, not a keyword,
/// and outside the prefixes the generated shim reserves for its own functions.
pub fn is_c_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !C_KEYWORDS.contains(&name)
        && !RESERVED_C_NAMES.contains(&name)
        && !name.starts_with(shim::SHIM_PREFIX)
        && !name.starts_with("dream_ctramp_")
        && !name.starts_with("DREAM_SHIM")
        && !name.contains("__body")
}

/// A library name passed to the linker: non-empty, and not a path.
pub fn is_link_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.contains('\0')
        && !name.contains(' ')
}

/// A C++ id-expression the shim can splice: `ident(::ident)*` plus one optional template
/// argument list. Rejects anything that could break out of the generated source.
pub fn is_cpp_id_expr(input: &str) -> bool {
    fn ident(bytes: &[u8], i: &mut usize) -> bool {
        let Some(c) = bytes.get(*i).copied() else {
            return false;
        };
        if !c.is_ascii_alphabetic() && c != b'_' {
            return false;
        }
        *i += 1;
        while bytes
            .get(*i)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            *i += 1;
        }
        true
    }
    fn expr(bytes: &[u8], i: &mut usize) -> bool {
        if !ident(bytes, i) {
            return false;
        }
        while bytes.get(*i..*i + 2) == Some(b"::") {
            *i += 2;
            if !ident(bytes, i) {
                return false;
            }
        }
        if bytes.get(*i) == Some(&b'<') {
            *i += 1;
            if !arg(bytes, i) {
                return false;
            }
            while bytes.get(*i) == Some(&b',') {
                *i += 1;
                while bytes.get(*i) == Some(&b' ') {
                    *i += 1;
                }
                if !arg(bytes, i) {
                    return false;
                }
            }
            if bytes.get(*i) != Some(&b'>') {
                return false;
            }
            *i += 1;
        }
        true
    }
    fn arg(bytes: &[u8], i: &mut usize) -> bool {
        if bytes.get(*i).is_some_and(|c| c.is_ascii_digit()) {
            while bytes.get(*i).is_some_and(|c| c.is_ascii_alphanumeric()) {
                *i += 1;
            }
            return true;
        }
        expr(bytes, i)
    }
    let bytes = input.as_bytes();
    let mut i = 0;
    expr(bytes, &mut i) && i == bytes.len()
}

/// One component of a generated shim symbol. Length-prefixed, with non-identifier bytes escaped,
/// so `kv-store` and `kv_store` (and `put` overload 2 versus a member named `put_2`) cannot meet.
fn encode_shim_component(name: &str) -> String {
    let mut enc = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            enc.push(c);
        } else {
            enc.push_str(&format!("U{:02x}", u32::from(c)));
        }
    }
    format!("{}x{enc}", name.chars().count())
}

/// The embedding API declared by `include/dream_embed.h`, preserved for native libraries and
/// programs whose reachable foreign code or exports can invoke it.
pub const EMBED_EXPORTS: [&str; 22] = [
    "dream_embed_abi_version",
    "dream_thread_attach",
    "dream_thread_detach",
    "dream_retain",
    "dream_release",
    "dream_callback_release",
    "dream_complete_foreign",
    "dream_set_panic_hook",
    "dream_set_platform",
    "dream_scheduler_pump",
    "dream_bytes_new",
    "dream_scalar_array_new",
    "dream_array_data",
    "dream_string_from_utf8",
    "dream_string_from_utf16",
    "dream_string_units",
    "dream_string_length",
    "dream_future_complete_u64",
    "dream_future_complete_ref",
    "dream_future_complete_error",
    "dream_future_on_cancel",
    "dream_post",
];

/// The stdlib callback class (`system.NativeCallback<F>`), passed to C as `(fn, void* user_data)`.
pub use crate::core_items::NATIVE_CALLBACK_NAME as NATIVE_CALLBACK_TYPE;

/// The generated C++ shim's `extern "C"` symbol for one `@cpp` member. Each component is
/// length-prefixed and escaped (see [`encode_shim_component`]), and the overload index is its own
/// component, so distinct declarations cannot produce the same symbol. `class` is empty for free
/// functions.
pub fn cpp_shim_symbol(set: &str, class: &str, member: &str, overload: usize) -> String {
    format!(
        "dream__{}__{}__{}__{overload}",
        encode_shim_component(set),
        encode_shim_component(class),
        encode_shim_component(member)
    )
}

#[cfg(test)]
mod tests {
    use super::{cpp_shim_symbol, is_c_identifier, is_cpp_id_expr, is_link_name};

    #[test]
    fn c_identifiers_exclude_keywords_and_shim_names() {
        assert!(is_c_identifier("sqlite3_open"));
        assert!(is_c_identifier("_aligned_malloc"));
        assert!(is_c_identifier(&cpp_shim_symbol("m", "C", "f", 0)));
        assert!(!is_c_identifier("struct"));
        assert!(!is_c_identifier("_Bool"));
        assert!(!is_c_identifier("asm"));
        assert!(!is_c_identifier("dream_ctramp_x"));
        assert!(!is_c_identifier("has__body"));
        assert!(!is_c_identifier("1abc"));
        assert!(!is_c_identifier("foo@8"));
        assert!(!is_c_identifier("dream_cs_x"));
        assert!(!is_c_identifier(""));
    }

    #[test]
    fn shim_symbols_are_c_identifiers() {
        assert_eq!(
            cpp_shim_symbol("kvstore", "Store", "get", 0),
            "dream__7xkvstore__5xStore__3xget__0"
        );
        assert_eq!(
            cpp_shim_symbol("kv-store", "Store", "put", 2),
            "dream__8xkvU2dstore__5xStore__3xput__2"
        );
        assert_ne!(
            cpp_shim_symbol("kv-store", "Store", "put", 0),
            cpp_shim_symbol("kv_store", "Store", "put", 0)
        );
        assert_ne!(
            cpp_shim_symbol("m", "C", "put", 2),
            cpp_shim_symbol("m", "C", "put_2", 0)
        );
        assert_eq!(
            cpp_shim_symbol("m", "", "version", 0),
            "dream__1xm__0x__7xversion__0"
        );
        assert!(is_cpp_id_expr("std::vector<int>"));
        assert!(is_cpp_id_expr("ns::Foo<ns::Bar<int>>"));
        assert!(!is_cpp_id_expr("foo(); system(\"rm\")"));
        assert!(!is_link_name("../lib"));
        assert!(is_link_name("sqlite3"));
    }
}
