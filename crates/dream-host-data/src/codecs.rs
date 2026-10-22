use serde_json::{Value, json};

pub(crate) fn failure(message: &str) -> String {
    json!({"error":message}).to_string()
}
pub(crate) fn request(input: &str) -> String {
    if input.len() > 16 * 1024 * 1024 {
        return failure("codec request exceeds 16 MiB");
    }
    let value: Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => return failure("invalid codec request"),
    };
    match execute(&value) {
        Ok(value) => value.to_string(),
        Err(error) => failure(&error),
    }
}
fn execute(value: &Value) -> Result<Value, String> {
    if value["action"]
        .as_str()
        .is_some_and(|v| v.starts_with("compress_"))
    {
        #[cfg(feature = "compression")]
        return crate::compression::request(value);
        #[cfg(not(feature = "compression"))]
        return Err("compression capability is not linked".into());
    }
    Err("unsupported codec operation".into())
}
