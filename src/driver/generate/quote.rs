/// JSON output uses the JSON escape vocabulary, including Unicode and control characters.
pub(super) fn json_string(value: &str) -> String {
    serde_json::Value::String(value.to_owned()).to_string()
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
