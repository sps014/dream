use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

enum Engine {
    Encode(Compress),
    Decode(Decompress),
}
struct Stream {
    engine: Engine,
    limit: u64,
    finished: bool,
    failed: bool,
}
#[derive(Default)]
struct Registry {
    next: i32,
    streams: BTreeMap<i32, Stream>,
}
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}

pub(crate) fn request(value: &Value) -> Result<Value, String> {
    let mut registry = registry().lock().unwrap_or_else(|e| e.into_inner());
    match value["action"].as_str() {
        Some("compress_create") => {
            if registry.streams.len() >= 128 || registry.next == i32::MAX {
                return Err("compression resource capacity exhausted".into());
            }
            let limit = value["limit"]
                .as_u64()
                .filter(|&n| n <= 1_073_741_824)
                .ok_or("invalid compression output limit")?;
            let gzip = match value["format"].as_str() {
                Some("gzip") => true,
                Some("zlib") => false,
                _ => return Err("unsupported compression format".into()),
            };
            let engine = match value["direction"].as_str() {
                Some("encode") => Engine::Encode(if gzip {
                    Compress::new_gzip(Compression::default(), 15)
                } else {
                    Compress::new(Compression::default(), true)
                }),
                Some("decode") => Engine::Decode(if gzip {
                    Decompress::new_gzip(15)
                } else {
                    Decompress::new(true)
                }),
                _ => return Err("invalid compression direction".into()),
            };
            registry.next += 1;
            let id = registry.next;
            registry.streams.insert(
                id,
                Stream {
                    engine,
                    limit,
                    finished: false,
                    failed: false,
                },
            );
            Ok(json!({"id":id}))
        }
        Some("compress_close") => {
            if let Some(id) = value["id"].as_i64().and_then(|v| i32::try_from(v).ok()) {
                registry.streams.remove(&id);
            }
            Ok(json!({}))
        }
        Some("compress_step") => {
            let id = value["id"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or("invalid compression handle")?;
            let stream = registry
                .streams
                .get_mut(&id)
                .ok_or("compression stream is closed")?;
            if stream.failed {
                return Err("compression stream is failed".into());
            }
            let result = stream.step(value);
            if result.is_err() {
                stream.failed = true;
            }
            result
        }
        _ => Err("unsupported compression operation".into()),
    }
}
impl Stream {
    fn step(&mut self, value: &Value) -> Result<Value, String> {
        let input = value["input"].as_str().ok_or("missing compression input")?;
        if input.len() > 131072 {
            return Err("compression chunks may not exceed 64 KiB".into());
        }
        let input = hex::decode(input).map_err(|_| "invalid compression input")?;
        let capacity = value["capacity"]
            .as_u64()
            .filter(|&n| n > 0 && n <= 65536)
            .ok_or("invalid compression output capacity")? as usize;
        let finish = value["finish"].as_bool().unwrap_or(false);
        if self.finished {
            if !input.is_empty() {
                return Err("trailing bytes after compressed stream".into());
            }
            return Ok(json!({"consumed":0,"output":"","finished":true}));
        }
        let mut output = vec![0; capacity];
        let (status, consumed, produced, total) = match &mut self.engine {
            Engine::Encode(engine) => {
                let (before_in, before_out) = (engine.total_in(), engine.total_out());
                let status = engine
                    .compress(
                        &input,
                        &mut output,
                        if finish {
                            FlushCompress::Finish
                        } else if value["flush"].as_bool().unwrap_or(false) {
                            FlushCompress::Sync
                        } else {
                            FlushCompress::None
                        },
                    )
                    .map_err(|e| e.to_string())?;
                (
                    status,
                    engine.total_in() - before_in,
                    engine.total_out() - before_out,
                    engine.total_out(),
                )
            }
            Engine::Decode(engine) => {
                let (before_in, before_out) = (engine.total_in(), engine.total_out());
                let status = engine
                    .decompress(
                        &input,
                        &mut output,
                        if finish {
                            FlushDecompress::Finish
                        } else {
                            FlushDecompress::None
                        },
                    )
                    .map_err(|e| e.to_string())?;
                (
                    status,
                    engine.total_in() - before_in,
                    engine.total_out() - before_out,
                    engine.total_out(),
                )
            }
        };
        if total > self.limit {
            return Err("compression output limit exceeded".into());
        }
        self.finished = status == Status::StreamEnd;
        if finish && !self.finished && consumed == 0 && produced == 0 {
            return Err("truncated or incomplete compressed stream".into());
        }
        output.truncate(produced as usize);
        Ok(json!({"consumed":consumed,"output":hex::encode(output),"finished":self.finished}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn open(direction: &str, limit: usize) -> i64 {
        request(&json!({"action":"compress_create","format":"gzip","direction":direction,"limit":limit})).unwrap()["id"].as_i64().unwrap()
    }
    fn step(id: i64, input: &[u8], finish: bool) -> Result<Value, String> {
        request(
            &json!({"action":"compress_step","id":id,"input":hex::encode(input),"finish":finish,"capacity":65536}),
        )
    }
    #[test]
    fn gzip_round_trip_limits_and_corruption() {
        let encoder = open("encode", 100000);
        let encoded = step(encoder, b"hello compression", true).unwrap();
        assert_eq!(encoded["finished"], true);
        let mut bytes = hex::decode(encoded["output"].as_str().unwrap()).unwrap();
        assert_eq!(&bytes[..2], &[31, 139]);
        let decoder = open("decode", 1000);
        let decoded = step(decoder, &bytes, true).unwrap();
        assert_eq!(
            hex::decode(decoded["output"].as_str().unwrap()).unwrap(),
            b"hello compression"
        );
        let limited = open("decode", 2);
        assert!(step(limited, &bytes, true).is_err());
        let truncated = open("decode", 1000);
        let first = step(truncated, &bytes[..5], true).unwrap();
        assert_eq!(first["finished"], false);
        assert!(step(truncated, &[], true).is_err());
        let corrupt = open("decode", 1000);
        let end = bytes.len() - 5;
        bytes[end] ^= 128;
        assert!(step(corrupt, &bytes, true).is_err());
        for id in [encoder, decoder, limited, truncated, corrupt] {
            request(&json!({"action":"compress_close","id":id})).unwrap();
        }
    }
}
