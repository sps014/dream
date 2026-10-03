//! Names shared by sema and MIR for the native C/C++ boundary.

/// The stdlib opaque pointer struct (`system.CPtr`): one `usize` field, passed to C by value.
pub const C_PTR_TYPE: &str = "CPtr";

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
    use super::cpp_shim_symbol;

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
