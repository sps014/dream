use super::{
    request::{self, BodySource},
    state::{self, Upload, error, lock, tables},
};
use crate::socket_operations::{Failure, Output};
use bytes::Bytes;
use dream_host_abi::wire::Reader;
use futures_util::{StreamExt, stream};
use http_body_util::BodyDataStream;
use serde_json::json;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
};
use tempfile::SpooledTempFile;

/// Starlette's `UploadFile` spool: parts up to this size stay in memory.
const SPOOL_BYTES: usize = 1 << 20;

struct Limits {
    max_files: u64,
    max_fields: u64,
    max_part_size: usize,
}

fn malformed(_: dream_host_abi::wire::Malformed) -> Failure {
    error("EINVAL", "malformed form operation")
}

fn limit(reader: &mut Reader<'_>, name: &str) -> Result<u32, Failure> {
    u32::try_from(reader.i32().map_err(malformed)?)
        .map_err(|_| error("EINVAL", format!("invalid form limit {name}")))
}

pub(super) async fn parse(id: i32, args: &[u8]) -> Result<Output, Failure> {
    let mut reader = Reader::new(args);
    let limits = Limits {
        max_files: limit(&mut reader, "max_files")?.into(),
        max_fields: limit(&mut reader, "max_fields")?.into(),
        max_part_size: usize::try_from(limit(&mut reader, "max_part_size")?)
            .map_err(|_| error("EINVAL", "invalid form limit max_part_size"))?,
    };
    reader.finish().map_err(malformed)?;
    let request = state::request(id)?;
    let content_type = request
        .head
        .headers
        .get(hyper::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let boundary = multer::parse_boundary(&content_type).map_err(|e| error("EINVAL", e))?;
    let mut multipart = match request::take_source(id).await? {
        BodySource::Stream(body) => multer::Multipart::new(BodyDataStream::new(body), boundary),
        BodySource::Cached(bytes) => multer::Multipart::new(
            stream::once(async move { Ok::<Bytes, std::io::Error>(bytes) }).boxed(),
            boundary,
        ),
    };
    let mut fields = Vec::new();
    let mut files = Vec::new();
    let mut handles = Vec::new();
    let result = async {
        while let Some(mut field) = multipart
            .next_field()
            .await
            .map_err(|e| error("EINVAL", e))?
        {
            let name = field.name().unwrap_or("").to_string();
            let Some(filename) = field.file_name().map(str::to_string) else {
                if fields.len() as u64 >= limits.max_fields {
                    return Err(error(
                        "EINVAL",
                        format!("too many fields; the maximum is {}", limits.max_fields),
                    ));
                }
                let mut text = Vec::new();
                while let Some(chunk) = field.chunk().await.map_err(|e| error("EINVAL", e))? {
                    if text.len() + chunk.len() > limits.max_part_size {
                        return Err(error(
                            "EINVAL",
                            format!(
                                "form field {name} exceeds the maximum part size of {} bytes",
                                limits.max_part_size
                            ),
                        ));
                    }
                    text.extend_from_slice(&chunk);
                }
                let text = String::from_utf8(text)
                    .map_err(|_| error("EINVAL", format!("form field {name} is not UTF-8")))?;
                fields.push(json!({"name":name,"value":text}));
                continue;
            };
            if files.len() as u64 >= limits.max_files {
                return Err(error(
                    "EINVAL",
                    format!("too many files; the maximum is {}", limits.max_files),
                ));
            }
            let content_type = field
                .content_type()
                .map(ToString::to_string)
                .unwrap_or_default();
            let part_headers = field
                .headers()
                .iter()
                .map(|(k, v)| [k.as_str().to_string(), String::from_utf8_lossy(v.as_bytes()).to_string()])
                .collect::<Vec<_>>();
            let upload: Upload = Arc::new(Mutex::new(SpooledTempFile::new(SPOOL_BYTES)));
            let handle = lock(tables())
                .uploads
                .insert(upload.clone())
                .ok_or_else(|| error("ELIMIT", "too many open uploaded files"))?;
            handles.push(handle);
            let mut size = 0u64;
            while let Some(chunk) = field.chunk().await.map_err(|e| error("EINVAL", e))? {
                size += chunk.len() as u64;
                let file = upload.clone();
                tokio::task::spawn_blocking(move || lock(&file).write_all(&chunk))
                    .await
                    .map_err(|e| error("EIO", e))?
                    .map_err(|e| error("EIO", e))?;
            }
            let file = upload.clone();
            tokio::task::spawn_blocking(move || lock(&file).seek(SeekFrom::Start(0)))
                .await
                .map_err(|e| error("EIO", e))?
                .map_err(|e| error("EIO", e))?;
            files.push(json!({"name":name,"filename":filename,"content_type":content_type,"headers":part_headers,"size":size,"handle":handle}));
        }
        Ok(())
    }
    .await;
    if let Err(failure) = result {
        let mut tables = lock(tables());
        for handle in handles {
            tables.uploads.remove(handle);
        }
        return Err(failure);
    }
    lock(&request.uploads).extend(handles);
    Ok(Output::bytes(
        json!({"fields":fields,"files":files})
            .to_string()
            .into_bytes(),
    ))
}

pub(super) async fn read(id: i32, args: &[u8]) -> Result<Output, Failure> {
    let mut reader = Reader::new(args);
    let max = reader.i32().map_err(malformed)?;
    reader.finish().map_err(malformed)?;
    let upload = state::upload(id)?;
    let bytes = tokio::task::spawn_blocking(move || {
        let mut file = lock(&upload);
        let mut bytes = Vec::new();
        if max < 0 {
            file.read_to_end(&mut bytes)?;
        } else {
            (&mut *file).take(max as u64).read_to_end(&mut bytes)?;
        }
        Ok::<_, std::io::Error>(bytes)
    })
    .await
    .map_err(|e| error("EIO", e))?
    .map_err(|e| error("EIO", e))?;
    Ok(Output::bytes(bytes))
}

pub(super) async fn seek(id: i32, args: &[u8]) -> Result<Output, Failure> {
    let mut reader = Reader::new(args);
    let offset = u64::try_from(reader.i64().map_err(malformed)?)
        .map_err(|_| error("EINVAL", "invalid seek offset"))?;
    reader.finish().map_err(malformed)?;
    let upload = state::upload(id)?;
    tokio::task::spawn_blocking(move || lock(&upload).seek(SeekFrom::Start(offset)))
        .await
        .map_err(|e| error("EIO", e))?
        .map_err(|e| error("EIO", e))?;
    Ok(Output::bytes(vec![]))
}

pub(crate) fn close(id: i32) {
    let upload = lock(tables()).uploads.remove(id);
    drop(upload);
}
