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
    #[cfg(feature = "csv")]
    {
        csv_request(value)
    }
    #[cfg(not(feature = "csv"))]
    Err("CSV capability is not linked".into())
}

#[cfg(feature = "csv")]
fn csv_request(value: &Value) -> Result<Value, String> {
    let delimiter = value["delimiter"]
        .as_u64()
        .filter(|&d| d > 0 && d < 128 && ![10, 13, 34].contains(&d))
        .ok_or("invalid CSV delimiter")? as u8;
    match value["action"].as_str() {
        Some("csv_read") => {
            let input = value["input"].as_str().ok_or("missing CSV input")?;
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .delimiter(delimiter)
                .from_reader(input.as_bytes());
            let mut records = reader.records();
            let record = records
                .next()
                .transpose()
                .map_err(|e| e.to_string())?
                .ok_or("missing CSV record")?;
            if records.next().is_some() {
                return Err("expected a single CSV record".into());
            }
            Ok(json!({"fields":record.iter().collect::<Vec<_>>()}))
        }
        Some("csv_write") => {
            let fields = value["fields"].as_array().ok_or("missing CSV fields")?;
            let fields = fields
                .iter()
                .map(|v| v.as_str().ok_or("CSV fields must be strings"))
                .collect::<Result<Vec<_>, _>>()?;
            let mut writer = csv::WriterBuilder::new()
                .delimiter(delimiter)
                .terminator(csv::Terminator::CRLF)
                .from_writer(Vec::new());
            writer.write_record(fields).map_err(|e| e.to_string())?;
            let output = writer.into_inner().map_err(|e| e.to_string())?;
            Ok(json!({"text":String::from_utf8(output).map_err(|e| e.to_string())?}))
        }
        _ => Err("unsupported codec operation".into()),
    }
}
#[cfg(all(test, feature = "csv"))]
mod tests {
    use super::*;
    #[test]
    fn quoted_records_and_writer_round_trip() {
        let encoded: Value = serde_json::from_str(&request(&json!({"action":"csv_write","delimiter":59,"fields":["a;b","say \"hi\"","line\nnext", ""]}).to_string())).unwrap();
        let decoded: Value = serde_json::from_str(&request(
            &json!({"action":"csv_read","delimiter":59,"input":encoded["text"]}).to_string(),
        ))
        .unwrap();
        assert_eq!(
            decoded["fields"],
            json!(["a;b", "say \"hi\"", "line\nnext", ""])
        );
        assert!(request("not JSON").contains("error"));
    }
}
