// Guest pointers come from the validated runtime C ABI, not arbitrary Rust callers.
#![allow(clippy::missing_safety_doc)]

use dream_host_abi::*;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unicodeNormalize(text: DreamPtr, form: i32) -> DreamPtr { unsafe {
    use unicode_normalization::UnicodeNormalization;
    let s = read_string(text);
    let out = match form {
        1 => s.nfd().collect::<String>(),
        2 => s.nfkc().collect::<String>(),
        3 => s.nfkd().collect::<String>(),
        _ => s.nfc().collect::<String>(),
    };
    alloc_string(&out)
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unicodeToLower(text: DreamPtr) -> DreamPtr { unsafe {
    alloc_string(&read_string(text).to_lowercase())
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unicodeToUpper(text: DreamPtr) -> DreamPtr { unsafe {
    alloc_string(&read_string(text).to_uppercase())
}}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unicodeGraphemes(text: DreamPtr) -> DreamPtr { unsafe {
    use unicode_segmentation::UnicodeSegmentation;
    let s = read_string(text);
    let parts: Vec<String> = s.graphemes(true).map(str::to_string).collect();
    alloc_string_array(&parts)
}}
