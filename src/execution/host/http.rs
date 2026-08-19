//! HTTP host functions (the `Dream` module behind `system.net` `HttpClient`). The buffered
//! `httpRequest*` functions perform the whole request synchronously (blocking `reqwest`) and
//! bridge the serialized response into Dream's async runtime. The streaming `httpRequestStream*`
//! functions keep the `reqwest::blocking::Response` in a handle table so `httpReadChunk` can
//! pull the body incrementally without buffering it all in memory.

use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use indexmap::IndexMap;

fn build_http_client(http2: bool) -> reqwest::blocking::Client {
    let mut builder = reqwest::blocking::Client::builder()
        .pool_max_idle_per_host(0)
        .pool_idle_timeout(Duration::from_secs(0))
        .tcp_keepalive(None)
        .connect_timeout(Duration::from_secs(10));
    if !http2 {
        builder = builder.http1_only();
    }
    builder.build().expect("reqwest HTTP client")
}

/// Fresh client per request so idle connection state is not reused across calls.
fn http_client(http_version: i32) -> reqwest::blocking::Client {
    build_http_client(http_version == 2)
}

fn http_transport_error(msg: &str) -> Vec<u8> {
    let mut out = b"0\n\n".to_vec(); // status 0 = transport error; body is the message
    out.extend_from_slice(msg.as_bytes());
    out
}

fn http_error_message(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timeout".to_string()
    } else {
        err.to_string()
    }
}

/// Runs `f` with an optional wall-clock budget. When `wall` is `None`, runs on the calling thread.
/// When set, runs on a worker and returns `None` if the budget elapses before `f` finishes; the
/// worker result is then discarded (never published to the caller).
fn with_http_wall<T, F>(wall: Option<Duration>, f: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let Some(wall) = wall else {
        return Some(f());
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(wall).ok()
}

fn response_head(response: &reqwest::blocking::Response) -> String {
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
    head
}

/// Performs one blocking HTTP request and serializes the whole response into the wire format shared
/// with `runtime/dream.js` (and parsed by `HttpResponse`): an ASCII head ("<status>\n" plus
/// "Name: value\n" lines), a blank line, then the raw body bytes. `body` is sent verbatim unless the
/// verb is GET/HEAD or it is empty. Network/protocol errors come back as a status `0` response whose
/// body is the error text. `timeout_ms == 0` means no timeout. `http_version` is `1` (default) or `2`.
pub(crate) fn perform_http(
    method: &str,
    url: &str,
    headers_json: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> Vec<u8> {
    let method = method.to_owned();
    let url = url.to_owned();
    let headers_json = headers_json.to_owned();
    let wall = if timeout_ms > 0 {
        Some(Duration::from_millis(timeout_ms as u64))
    } else {
        None
    };
    match with_http_wall(wall, move || {
        match build_request(&method, &url, &headers_json, body, timeout_ms, http_version).send() {
            Ok(response) => {
                let mut out = response_head(&response).into_bytes();
                if let Ok(body_bytes) = response.bytes() {
                    out.extend_from_slice(&body_bytes);
                }
                out
            }
            Err(e) => http_transport_error(&http_error_message(&e)),
        }
    }) {
        Some(out) => out,
        None => http_transport_error("timeout"),
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
pub(crate) fn open_http_stream(
    method: &str,
    url: &str,
    headers_json: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> Vec<u8> {
    let method = method.to_owned();
    let url = url.to_owned();
    let headers_json = headers_json.to_owned();
    let wall = if timeout_ms > 0 {
        Some(Duration::from_millis(timeout_ms as u64))
    } else {
        None
    };
    // Send the Response back to this thread before publishing a stream handle, so a wall timeout
    // cannot leave an orphan entry in the handle table.
    match with_http_wall(wall, move || {
        build_request(&method, &url, &headers_json, body, timeout_ms, http_version)
            .send()
            .map_err(|e| http_error_message(&e))
    }) {
        None => http_transport_error("timeout"),
        Some(Err(msg)) => http_transport_error(&msg),
        Some(Ok(response)) => {
            let mut head = response_head(&response);
            let id = NEXT_STREAM_HANDLE.fetch_add(1, Ordering::Relaxed);
            let mut table = stream_handles().lock().unwrap_or_else(|e| e.into_inner());
            table.insert(id, response);
            head.push_str(&id.to_string());
            head.into_bytes()
        }
    }
}

/// Reads up to `max_bytes` from an open stream handle. Wire: `"data\n<bytes>"` | `"eof\n"` |
/// `"error\n<message>"`.
pub(crate) fn http_read_chunk(handle: i32, max_bytes: i32) -> Vec<u8> {
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

pub(crate) fn http_close_stream(handle: i32) -> i32 {
    let mut table = stream_handles().lock().unwrap_or_else(|e| e.into_inner());
    table.shift_remove(&(handle as u32)).is_some() as i32
}
