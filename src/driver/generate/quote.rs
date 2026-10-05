/// JSON strings and Dream literals have different escape vocabularies: Dream does not
/// interpret JSON's Unicode, backspace or form-feed escapes.
pub(super) fn json_string(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
}

pub(super) fn dream_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_strings_round_trip_control_characters_and_unicode() {
        let value = "quote\" slash\\\n\r\t\0\u{8}\u{c}λ";
        assert_eq!(
            serde_json::from_str::<String>(&json_string(value)).unwrap(),
            value
        );
    }
}
