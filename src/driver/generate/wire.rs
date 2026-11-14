//! Binary snapshot and result codec shared with `system.codegen` (`gen_wire.dream`).
//!
//! `DGEN`, one version byte, then one value. Little-endian lengths. Objects keep insertion
//! order, which is struct field order for values produced by serde.
//!
//! Tags: 0 null, 1 bool, 2 i64, 3 decimal text, 4 UTF-8 string, 5 array, 6 object.

#[cfg(test)]
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Number, Value};

const MAGIC: &[u8] = b"DGEN";
const VERSION: u8 = 1;

pub(crate) fn encode_json(text: &str) -> Result<Vec<u8>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    encode_value(&value)
}

pub(crate) fn encode_value(value: &Value) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(MAGIC.len() + 1 + 64);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    write_value(&mut out, value)?;
    Ok(out)
}

pub(crate) fn decode_value(bytes: &[u8]) -> Result<Value, String> {
    let mut at = 0;
    expect_header(bytes, &mut at)?;
    let value = read_value(bytes, &mut at)?;
    if at != bytes.len() {
        return Err("trailing bytes after generator value".to_string());
    }
    Ok(value)
}

pub(crate) fn decode_typed<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let value = decode_value(bytes)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) fn encode_typed<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    encode_value(&value)
}

fn expect_header(bytes: &[u8], at: &mut usize) -> Result<(), String> {
    if bytes.len() < MAGIC.len() + 1 || &bytes[..MAGIC.len()] != MAGIC {
        return Err("generator value is not a DGEN payload".to_string());
    }
    if bytes[MAGIC.len()] != VERSION {
        return Err(format!(
            "generator wire version {} is not {VERSION}",
            bytes[MAGIC.len()]
        ));
    }
    *at = MAGIC.len() + 1;
    Ok(())
}

fn write_value(out: &mut Vec<u8>, value: &Value) -> Result<(), String> {
    match value {
        Value::Null => out.push(0),
        Value::Bool(v) => {
            out.push(1);
            out.push(u8::from(*v));
        }
        Value::Number(n) => write_number(out, n)?,
        Value::String(s) => {
            out.push(4);
            write_bytes(out, s.as_bytes())?;
        }
        Value::Array(items) => {
            out.push(5);
            write_len(out, items.len())?;
            for item in items {
                write_value(out, item)?;
            }
        }
        Value::Object(map) => write_object(out, map)?,
    }
    Ok(())
}

fn write_number(out: &mut Vec<u8>, n: &Number) -> Result<(), String> {
    if let Some(v) = n.as_i64() {
        out.push(2);
        out.extend_from_slice(&v.to_le_bytes());
        return Ok(());
    }
    if let Some(v) = n.as_u64()
        && let Ok(v) = i64::try_from(v)
    {
        out.push(2);
        out.extend_from_slice(&v.to_le_bytes());
        return Ok(());
    }
    out.push(3);
    write_bytes(out, n.to_string().as_bytes())
}

fn write_object(out: &mut Vec<u8>, map: &Map<String, Value>) -> Result<(), String> {
    out.push(6);
    write_len(out, map.len())?;
    for (key, value) in map {
        write_bytes(out, key.as_bytes())?;
        write_value(out, value)?;
    }
    Ok(())
}

fn write_len(out: &mut Vec<u8>, len: usize) -> Result<(), String> {
    let len = u32::try_from(len).map_err(|_| "generator value is too large".to_string())?;
    out.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    write_len(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn read_value(bytes: &[u8], at: &mut usize) -> Result<Value, String> {
    let tag = read_u8(bytes, at)?;
    match tag {
        0 => Ok(Value::Null),
        1 => Ok(Value::Bool(read_u8(bytes, at)? != 0)),
        2 => Ok(Value::Number(read_i64(bytes, at)?.into())),
        3 => {
            let text = read_string(bytes, at)?;
            let n = text
                .parse::<Number>()
                .map_err(|_| format!("invalid decimal '{text}'"))?;
            Ok(Value::Number(n))
        }
        4 => Ok(Value::String(read_string(bytes, at)?)),
        5 => {
            let n = read_len(bytes, at)?;
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(read_value(bytes, at)?);
            }
            Ok(Value::Array(items))
        }
        6 => {
            let n = read_len(bytes, at)?;
            let mut map = Map::new();
            for _ in 0..n {
                let key = read_string(bytes, at)?;
                let value = read_value(bytes, at)?;
                map.insert(key, value);
            }
            Ok(Value::Object(map))
        }
        other => Err(format!("unknown generator wire tag {other}")),
    }
}

fn read_string(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    let n = read_len(bytes, at)?;
    let start = *at;
    let end = start
        .checked_add(n)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| "truncated generator string".to_string())?;
    *at = end;
    String::from_utf8(bytes[start..end].to_vec()).map_err(|e| e.to_string())
}

fn read_len(bytes: &[u8], at: &mut usize) -> Result<usize, String> {
    let word = read_u32(bytes, at)?;
    Ok(word as usize)
}

fn read_u32(bytes: &[u8], at: &mut usize) -> Result<u32, String> {
    if bytes.len().saturating_sub(*at) < 4 {
        return Err("truncated generator value".to_string());
    }
    let n = u32::from_le_bytes(bytes[*at..*at + 4].try_into().unwrap());
    *at += 4;
    Ok(n)
}

fn read_i64(bytes: &[u8], at: &mut usize) -> Result<i64, String> {
    if bytes.len().saturating_sub(*at) < 8 {
        return Err("truncated generator value".to_string());
    }
    let n = i64::from_le_bytes(bytes[*at..*at + 8].try_into().unwrap());
    *at += 8;
    Ok(n)
}

fn read_u8(bytes: &[u8], at: &mut usize) -> Result<u8, String> {
    let b = *bytes.get(*at).ok_or("truncated generator value")?;
    *at += 1;
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::generate::model::{GenResult, Output, Snapshot};

    #[test]
    fn roundtrips_nested_values_and_rejects_trailing_bytes() {
        let value = serde_json::json!({
            "version": 3,
            "ok": true,
            "missing": null,
            "n": -5,
            "wide": 9_223_372_036_854_775_808_u64,
            "items": ["a", {"k": 1}]
        });
        let bytes = encode_value(&value).unwrap();
        assert_eq!(decode_value(&bytes).unwrap(), value);
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(decode_value(&extra).is_err());
    }

    #[test]
    fn snapshot_and_result_survive_the_wire() {
        let mut snapshot = Snapshot {
            version: 3,
            generator: "demo".into(),
            ..Snapshot::default()
        };
        snapshot
            .options
            .insert("name".into(), serde_json::json!("dream"));
        let bytes = encode_typed(&snapshot).unwrap();
        let back: Snapshot = decode_typed(&bytes).unwrap();
        assert_eq!(back, snapshot);

        let result = GenResult {
            version: 3,
            generator: "demo".into(),
            outputs: vec![Output::File {
                path: "a.dream".into(),
                source: "fun main(): void {}".into(),
            }],
            ..GenResult::default()
        };
        let bytes = encode_typed(&result).unwrap();
        assert_eq!(decode_typed::<GenResult>(&bytes).unwrap(), result);
    }
}
