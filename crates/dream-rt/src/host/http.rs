use crate::guest;
use indexmap::IndexMap;
use std::io::Read;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

fn client(http2: bool) -> reqwest::blocking::Client {
    let mut b = reqwest::blocking::Client::builder()
        .pool_max_idle_per_host(0)
        .connect_timeout(Duration::from_secs(10));
    if !http2 {
        b = b.http1_only();
    }
    b.build().expect("reqwest")
}

fn transport_error(msg: &str) -> Vec<u8> {
    let mut out = b"0\n\n".to_vec();
    out.extend_from_slice(msg.as_bytes());
    out
}

fn err_msg(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timeout".into()
    } else {
        err.to_string()
    }
}

fn with_wall<T, F>(wall: Option<Duration>, f: F) -> Option<T>
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
    let mut head = format!("{}\n", response.status().as_u16());
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
    let mut builder = client(http_version == 2).request(http_method, url);
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

fn perform(
    method: &str,
    url: &str,
    headers: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> Vec<u8> {
    let method = method.to_owned();
    let url = url.to_owned();
    let headers = headers.to_owned();
    let wall = (timeout_ms > 0).then(|| Duration::from_millis(timeout_ms as u64));
    match with_wall(wall, move || {
        match build_request(&method, &url, &headers, body, timeout_ms, http_version).send() {
            Ok(response) => {
                let mut out = response_head(&response).into_bytes();
                if let Ok(b) = response.bytes() {
                    out.extend_from_slice(&b);
                }
                out
            }
            Err(e) => transport_error(&err_msg(&e)),
        }
    }) {
        Some(out) => out,
        None => transport_error("timeout"),
    }
}

fn streams() -> &'static Mutex<IndexMap<u32, reqwest::blocking::Response>> {
    static H: OnceLock<Mutex<IndexMap<u32, reqwest::blocking::Response>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT: AtomicU32 = AtomicU32::new(1);

fn open_stream(
    method: &str,
    url: &str,
    headers: &str,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
) -> Vec<u8> {
    let method = method.to_owned();
    let url = url.to_owned();
    let headers = headers.to_owned();
    let wall = (timeout_ms > 0).then(|| Duration::from_millis(timeout_ms as u64));
    match with_wall(wall, move || {
        build_request(&method, &url, &headers, body, timeout_ms, http_version)
            .send()
            .map_err(|e| err_msg(&e))
    }) {
        None => transport_error("timeout"),
        Some(Err(msg)) => transport_error(&msg),
        Some(Ok(response)) => {
            let mut head = response_head(&response);
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            streams()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id, response);
            head.push_str(&id.to_string());
            head.into_bytes()
        }
    }
}

fn read_chunk(handle: i32, max_bytes: i32) -> Vec<u8> {
    let mut table = streams().lock().unwrap_or_else(|e| e.into_inner());
    let Some(response) = table.get_mut(&(handle as u32)) else {
        return b"error\nstream not found".to_vec();
    };
    let cap = (max_bytes.max(1) as usize).min(1 << 20);
    let mut buf = vec![0u8; cap];
    match response.read(&mut buf) {
        Ok(0) => {
            drop(table);
            streams()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .shift_remove(&(handle as u32));
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

#[no_mangle]
pub extern "C" fn dream_http_request(
    url: i32,
    method: i32,
    headers: i32,
    body: i32,
    timeout_ms: i32,
    http_version: i32,
) -> i32 {
    let body = guest::read_string(body).into_bytes();
    http_do(url, method, headers, body, timeout_ms, http_version, false)
}

#[no_mangle]
pub extern "C" fn dream_http_request_bytes(
    url: i32,
    method: i32,
    headers: i32,
    body: i32,
    timeout_ms: i32,
    http_version: i32,
) -> i32 {
    http_do(url, method, headers, guest::read_bytes(body), timeout_ms, http_version, false)
}

#[no_mangle]
pub extern "C" fn dream_http_request_stream(
    url: i32,
    method: i32,
    headers: i32,
    body: i32,
    timeout_ms: i32,
    http_version: i32,
) -> i32 {
    let body = guest::read_string(body).into_bytes();
    http_do(url, method, headers, body, timeout_ms, http_version, true)
}

#[no_mangle]
pub extern "C" fn dream_http_request_stream_bytes(
    url: i32,
    method: i32,
    headers: i32,
    body: i32,
    timeout_ms: i32,
    http_version: i32,
) -> i32 {
    http_do(url, method, headers, guest::read_bytes(body), timeout_ms, http_version, true)
}

fn http_do(
    url: i32,
    method: i32,
    headers: i32,
    body: Vec<u8>,
    timeout_ms: i32,
    http_version: i32,
    stream: bool,
) -> i32 {
    let url = guest::read_string(url);
    let method = guest::read_string(method);
    let headers = guest::read_string(headers);
    let bytes = if stream {
        open_stream(&method, &url, &headers, body, timeout_ms, http_version)
    } else {
        perform(&method, &url, &headers, body, timeout_ms, http_version)
    };
    guest::write_bytes(&bytes)
}

#[no_mangle]
pub extern "C" fn dream_http_read_chunk(handle: i32, max_bytes: i32) -> i32 {
    guest::write_bytes(&read_chunk(handle, max_bytes))
}

#[no_mangle]
pub extern "C" fn dream_http_close_stream(handle: i32) -> i32 {
    streams()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .shift_remove(&(handle as u32))
        .is_some() as i32
}
