use crate::guest;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

fn normalize(text: &str, form: i32) -> String {
    match form {
        1 => text.nfd().collect(),
        2 => text.nfkc().collect(),
        3 => text.nfkd().collect(),
        _ => text.nfc().collect(),
    }
}

#[no_mangle]
pub extern "C" fn dream_unicode_normalize(ptr: i32, form: i32) -> i32 {
    guest::intern(&normalize(&guest::read_string(ptr), form))
}

#[no_mangle]
pub extern "C" fn dream_unicode_to_lower(ptr: i32) -> i32 {
    guest::intern(&guest::read_string(ptr).to_lowercase())
}

#[no_mangle]
pub extern "C" fn dream_unicode_to_upper(ptr: i32) -> i32 {
    guest::intern(&guest::read_string(ptr).to_uppercase())
}

#[no_mangle]
pub extern "C" fn dream_unicode_graphemes(ptr: i32) -> i32 {
    let text = guest::read_string(ptr);
    let g: Vec<String> = text.graphemes(true).map(str::to_string).collect();
    guest::write_string_array(&g)
}
