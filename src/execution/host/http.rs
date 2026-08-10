//! HTTP host functions (the `Dream` module behind `system.net` `HttpClient`). The buffered
//! `httpRequest*` functions perform the whole request synchronously (blocking `reqwest`) and
//! bridge the serialized response into Dream's async runtime. The streaming `httpRequestStream*`
//! functions instead keep the `reqwest::blocking::Response` (which itself streams off the socket
//! on demand) in a handle table — the same handle-table pattern `net.rs`/`process.rs` use — so
//! `httpReadChunk` can pull the body incrementally without buffering it all in memory.

use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use indexmap::IndexMap;
use wasmtime::*;

use super::memory::{read_arg_bytes, read_arg_string, resolve_host_future_bytes};

/// Shared blocking clients (process-wide; per-request timeouts use `RequestBuilder::timeout`).
/// Default is HTTP/1.1 — Google fronts (youtube.com → googlevideo.com) can leave a shared HTTP/2
/// pool hung so later CDN GETs never complete. `http_version == 2` opts into a separate HTTP/2
/// client so that path cannot poison the default pool.
fn http_client(http_version: i32) -> &'static reqwest::blocking::Client {
    if http_version == 2 {
        static HTTP2: OnceLock<reqwest::blocking::Client> = OnceLock::new();
        HTTP2.get_or_init(reqwest::blocking::Client::new)
    } else {
        static HTTP1: OnceLock<reqwest::blocking::Client> = OnceLock::new();
        HTTP1.get_or_init(|| {
            reqwest::blocking::Client::builder()
                .http1_only()
                .build()
                .expect("reqwest HTTP/1.1 client")
        })
    }
}

/// Performs one blocking HTTP request and serializes the whole response into the wire format shared
/// with `runtime/dream.js` (and parsed by `HttpResponse`): an ASCII head ("<status>\n" plus
/// "Name: value\n" lines), a blank line, then the raw body bytes. `body` is sent verbatim unless the
/// verb is GET/HEAD or it is empty. Network/protocol errors come back as a status `0` response whose
/// body is the error text. `timeout_ms == 0` means no timeout. `http_version` is `1` (default) or `2`.
fn perform_http(
    method: &str,
    url: &str,
    headers_json: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> Vec<u8> {
    match build_request(method, url, headers_json, body, timeout_ms, http_version).send() {
        Ok(response) => {
            let status = response.status().as_u16();
            let mut head = format!("{}\n", status);
            for (name, value) in response.headers().iter() {
                if let Ok(v) = value.to_str() {
                    head.push_str(name.as_str());
                    head.push_str(": ");
                    head.push_str(v);
                    head.push('\n');
                }
            }
            head.push('\n'); // blank line separating head from body
            let mut out = head.into_bytes();
            if let Ok(body_bytes) = response.bytes() {
                out.extend_from_slice(&body_bytes);
            }
            out
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                "timeout".to_string()
            } else {
                e.to_string()
            };
            let mut out = b"0\n\n".to_vec(); // status 0 = transport error; body is the message
            out.extend_from_slice(msg.as_bytes());
            out
        }
    }
}

/// Builds a `reqwest::blocking::RequestBuilder` shared by the buffered and streaming request
/// paths, applying method/headers/timeout/body identically.
fn build_request(
    method: &str,
    url: &str,
    headers_json: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> reqwest::blocking::RequestBuilder {
    let verb = method.to_uppercase();
    let http_method = reqwest::Method::from_bytes(verb.as_bytes()).unwrap_or(reqwest::Method::GET);

    let mut builder = http_client(http_version).request(http_method, url);
    if timeout_ms > 0 {
        builder = builder.timeout(Duration::from_millis(timeout_ms as u64));
    }

    if !headers_json.is_empty() {
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(headers_json)
        {
            for (name, value) in map.iter() {
                if let Some(v) = value.as_str() {
                    builder = builder.header(name.as_str(), v);
                }
            }
        }
    }

    if !body.is_empty() && verb != "GET" && verb != "HEAD" {
        builder = builder.body(body);
    }
    builder
}

fn stream_handles() -> &'static Mutex<IndexMap<u32, reqwest::blocking::Response>> {
    static HANDLES: OnceLock<Mutex<IndexMap<u32, reqwest::blocking::Response>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_STREAM_HANDLE: AtomicU32 = AtomicU32::new(1);

/// Opens a request and returns just the head (status + headers), keeping the response body
/// unread in a handle table for `http_read_chunk` to pull incrementally. Wire format matches
/// [`perform_http`] exactly except the "body" is the decimal handle id, so `HttpResponse`'s
/// existing head parser can be reused unchanged on the Dream side (see `wrap_stream` in
/// `http_client.dream`).
fn open_http_stream(
    method: &str,
    url: &str,
    headers_json: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> Vec<u8> {
    match build_request(method, url, headers_json, body, timeout_ms, http_version).send() {
        Ok(response) => {
            let status = response.status().as_u16();
            let mut head = format!("{}\n", status);
            for (name, value) in response.headers().iter() {
                if let Ok(v) = value.to_str() {
                    head.push_str(name.as_str());
                    head.push_str(": ");
                    head.push_str(v);
                    head.push('\n');
                }
            }
            head.push('\n');
            let id = NEXT_STREAM_HANDLE.fetch_add(1, Ordering::Relaxed);
            let mut table = stream_handles().lock().unwrap_or_else(|e| e.into_inner());
            table.insert(id, response);
            head.push_str(&id.to_string());
            head.into_bytes()
        }
        Err(e) => {
            let msg = if e.is_timeout() {
                "timeout".to_string()
            } else {
                e.to_string()
            };
            let mut out = b"0\n\n".to_vec();
            out.extend_from_slice(msg.as_bytes());
            out
        }
    }
}

/// Reads up to `max_bytes` from an open stream handle. Wire: `"data\n<bytes>"` | `"eof\n"` |
/// `"error\n<message>"`.
fn http_read_chunk(handle: i32, max_bytes: i32) -> Vec<u8> {
    let mut table = stream_handles().lock().unwrap_or_else(|e| e.into_inner());
    let Some(response) = table.get_mut(&(handle as u32)) else {
        return b"error\nstream not found".to_vec();
    };
    let cap = (max_bytes.max(1) as usize).min(1 << 20);
    let mut buf = vec![0u8; cap];
    match response.read(&mut buf) {
        Ok(0) => {
            drop(table);
            let mut table = stream_handles().lock().unwrap_or_else(|e| e.into_inner());
            table.shift_remove(&(handle as u32));
            b"eof\n".to_vec()
        }
        Ok(n) => {
            let mut out = b"data\n".to_vec();
            out.extend_from_slice(&buf[..n]);
            out
        }
        Err(e) => {
            let mut out = b"error\n".to_vec();
            out.extend_from_slice(e.to_string().as_bytes());
            out
        }
    }
}

fn http_close_stream(handle: i32) -> i32 {
    let mut table = stream_handles().lock().unwrap_or_else(|e| e.into_inner());
    table.shift_remove(&(handle as u32)).is_some() as i32
}

/// Registers the HTTP host functions on `linker`. `httpRequest` takes a text body; `httpRequestBytes`
/// takes a binary `char[]` body. Both take `timeout_ms` (`0` = none) and `http_version` (`1` =
/// HTTP/1.1 default, `2` = HTTP/2). `httpRequestStream`/`httpRequestStreamBytes` open a stream
/// handle instead of buffering the body; `httpReadChunk`/`httpCloseStream` operate on that handle.
pub fn link_http_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "Dream",
        "httpRequest",
        |mut caller: Caller<'_, ()>,
         url_ptr: i32,
         method_ptr: i32,
         headers_ptr: i32,
         body_ptr: i32,
         timeout_ms: i32,
         http_version: i32|
         -> Result<i32> {
            let url = read_arg_string(&mut caller, url_ptr)?;
            let method = read_arg_string(&mut caller, method_ptr)?;
            let headers = read_arg_string(&mut caller, headers_ptr)?;
            let body = read_arg_string(&mut caller, body_ptr)?.into_bytes();
            let response = perform_http(&method, &url, &headers, body, timeout_ms, http_version);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "httpRequestBytes",
        |mut caller: Caller<'_, ()>,
         url_ptr: i32,
         method_ptr: i32,
         headers_ptr: i32,
         body_ptr: i32,
         timeout_ms: i32,
         http_version: i32|
         -> Result<i32> {
            let url = read_arg_string(&mut caller, url_ptr)?;
            let method = read_arg_string(&mut caller, method_ptr)?;
            let headers = read_arg_string(&mut caller, headers_ptr)?;
            let body = read_arg_bytes(&mut caller, body_ptr)?;
            let response = perform_http(&method, &url, &headers, body, timeout_ms, http_version);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "httpRequestStream",
        |mut caller: Caller<'_, ()>,
         url_ptr: i32,
         method_ptr: i32,
         headers_ptr: i32,
         body_ptr: i32,
         timeout_ms: i32,
         http_version: i32|
         -> Result<i32> {
            let url = read_arg_string(&mut caller, url_ptr)?;
            let method = read_arg_string(&mut caller, method_ptr)?;
            let headers = read_arg_string(&mut caller, headers_ptr)?;
            let body = read_arg_string(&mut caller, body_ptr)?.into_bytes();
            let response =
                open_http_stream(&method, &url, &headers, body, timeout_ms, http_version);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "httpRequestStreamBytes",
        |mut caller: Caller<'_, ()>,
         url_ptr: i32,
         method_ptr: i32,
         headers_ptr: i32,
         body_ptr: i32,
         timeout_ms: i32,
         http_version: i32|
         -> Result<i32> {
            let url = read_arg_string(&mut caller, url_ptr)?;
            let method = read_arg_string(&mut caller, method_ptr)?;
            let headers = read_arg_string(&mut caller, headers_ptr)?;
            let body = read_arg_bytes(&mut caller, body_ptr)?;
            let response =
                open_http_stream(&method, &url, &headers, body, timeout_ms, http_version);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "httpReadChunk",
        |mut caller: Caller<'_, ()>, handle: i32, max_bytes: i32| -> Result<i32> {
            let response = http_read_chunk(handle, max_bytes);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;

    linker.func_wrap(
        "Dream",
        "httpCloseStream",
        |_: Caller<'_, ()>, handle: i32| -> i32 { http_close_stream(handle) },
    )?;

    Ok(())
}
