//! Native HTTP server host (`system.webapi` / `WebApp`): hyper + tokio on a dedicated runtime
//! thread. The Dream cooperative loop accepts requests via `@async_host` and writes responses
//! through a handle table.

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use indexmap::IndexMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use tokio::net::TcpStream;
use tokio::sync::oneshot;

const MAX_BODY: usize = 16 * 1024 * 1024;

struct PendingReq {
    method: String,
    path: String,
    query: String,
    headers_json: String,
    body: Vec<u8>,
    resp_tx: oneshot::Sender<(u16, String, Vec<u8>)>,
}

struct LiveReq {
    method: String,
    path: String,
    query: String,
    headers_json: String,
    body: Vec<u8>,
    resp_tx: oneshot::Sender<(u16, String, Vec<u8>)>,
}

struct ServerSlot {
    incoming: std::sync::Arc<Mutex<mpsc::Receiver<PendingReq>>>,
    shutdown: std::sync::Arc<AtomicBool>,
}

struct Tables {
    next_server: AtomicU32,
    next_req: AtomicU32,
    servers: Mutex<IndexMap<u32, ServerSlot>>,
    requests: Mutex<IndexMap<u32, LiveReq>>,
    waits: Mutex<IndexMap<u32, mpsc::Sender<()>>>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| Tables {
        next_server: AtomicU32::new(1),
        next_req: AtomicU32::new(1),
        servers: Mutex::new(IndexMap::new()),
        requests: Mutex::new(IndexMap::new()),
        waits: Mutex::new(IndexMap::new()),
    })
}

fn headers_to_json(req: &Request<Incoming>) -> String {
    let mut map = serde_json::Map::new();
    for (name, value) in req.headers() {
        if let Ok(v) = value.to_str() {
            map.insert(
                name.as_str().to_string(),
                serde_json::Value::String(v.to_string()),
            );
        }
    }
    serde_json::Value::Object(map).to_string()
}

fn encode_listen_ok(handle: u32, port: u16) -> Vec<u8> {
    format!("ok\n{handle}\n{port}\n").into_bytes()
}

fn encode_err(msg: &str) -> Vec<u8> {
    format!("err\n{msg}\n").into_bytes()
}

fn encode_accept_ok(id: u32, live: &LiveReq) -> Vec<u8> {
    format!(
        "ok\n{}\n{}\n{}\n{}\n{}\n",
        id, live.method, live.path, live.query, live.headers_json
    )
    .into_bytes()
}

fn encode_shutdown() -> Vec<u8> {
    b"shutdown\n".to_vec()
}

/// Bind `host:port` (`port == 0` picks an ephemeral port). Returns `ok\nhandle\nport\n` or `err\n…`.
pub(crate) fn listen(host: &str, port: i32) -> Vec<u8> {
    let port = if port < 0 { 0 } else { port as u16 };
    let addr = format!("{host}:{port}");
    let listener = match std::net::TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => return encode_err(&e.to_string()),
    };
    let bound = match listener.local_addr() {
        Ok(a) => a.port(),
        Err(e) => return encode_err(&e.to_string()),
    };
    let _ = listener.set_nonblocking(true);
    let (tx, rx) = mpsc::channel::<PendingReq>();
    let shutdown = std::sync::Arc::new(AtomicBool::new(false));
    let shutdown_loop = shutdown.clone();
    let id = tables().next_server.fetch_add(1, Ordering::Relaxed);
    tables().servers.lock().expect("servers").insert(
        id,
        ServerSlot {
            incoming: std::sync::Arc::new(Mutex::new(rx)),
            shutdown: shutdown.clone(),
        },
    );
    std::thread::Builder::new()
        .name("dream-webapi-listen".into())
        .spawn(move || loop {
            if shutdown_loop.load(Ordering::Relaxed) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let tx = tx.clone();
                    let shutdown_conn = shutdown_loop.clone();
                    let _ = std::thread::Builder::new()
                        .name("dream-webapi-conn".into())
                        .spawn(move || serve_connection(stream, tx, shutdown_conn));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(_) => {
                    if shutdown_loop.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
            }
        })
        .expect("webapi listen thread");
    encode_listen_ok(id, bound)
}

fn serve_connection(
    stream: std::net::TcpStream,
    tx: mpsc::Sender<PendingReq>,
    shutdown_conn: std::sync::Arc<AtomicBool>,
) {
    let _ = stream.set_nonblocking(true);
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(_) => return,
    };
    let _ = rt.block_on(async move {
        let tstream = match TcpStream::from_std(stream) {
            Ok(s) => s,
            Err(_) => return,
        };
        let io = TokioIo::new(tstream);
        let svc = service_fn(move |req: Request<Incoming>| {
            let tx = tx.clone();
            let shutdown_conn = shutdown_conn.clone();
            async move {
                if shutdown_conn.load(Ordering::Relaxed) {
                    return Ok::<_, hyper::Error>(
                        Response::builder()
                            .status(StatusCode::SERVICE_UNAVAILABLE)
                            .body(Full::new(Bytes::from_static(b"shutting down")))
                            .expect("response"),
                    );
                }
                serve_one(req, tx).await
            }
        });
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(io, svc)
            .await;
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn listen_loopback_ok() {
        let w = super::listen("127.0.0.1", 0);
        let s = String::from_utf8_lossy(&w);
        assert!(s.starts_with("ok\n"), "listen wire: {}", s);
        let handle: i32 = s.lines().nth(1).unwrap().parse().unwrap();
        super::shutdown(handle);
    }

    #[test]
    fn accept_respond_roundtrip() {
        let w = super::listen("127.0.0.1", 0);
        let s = String::from_utf8(w).expect("utf8");
        let mut lines = s.lines();
        assert_eq!(lines.next(), Some("ok"));
        let handle: i32 = lines.next().unwrap().parse().unwrap();
        let port: u16 = lines.next().unwrap().parse().unwrap();
        let url = format!("http://127.0.0.1:{port}/health");
        let worker = std::thread::spawn(move || {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap()
                .get(&url)
                .send()
                .map(|r| (r.status().as_u16(), r.text().unwrap_or_default()))
                .map_err(|e| e.to_string())
        });
        let acc = super::accept(handle);
        let a = String::from_utf8_lossy(&acc);
        assert!(a.starts_with("ok\n"), "accept wire: {}", a);
        let id: i32 = a.lines().nth(1).unwrap().parse().unwrap();
        assert_eq!(super::respond(id, 200, "{}", b"ok".to_vec()), 1);
        let (status, body) = worker.join().unwrap().expect("client");
        assert_eq!(status, 200);
        assert_eq!(body, "ok");
        super::shutdown(handle);
    }
}

async fn serve_one(
    req: Request<Incoming>,
    tx: mpsc::Sender<PendingReq>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let method = req.method().as_str().to_string();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().unwrap_or("").to_string();
    let headers_json = headers_to_json(&req);
    let content_len = req
        .headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let chunked = req
        .headers()
        .get(hyper::header::TRANSFER_ENCODING)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.to_ascii_lowercase().contains("chunked"));
    let body = if content_len == 0 && !chunked {
        Bytes::new()
    } else {
        req.collect().await?.to_bytes()
    };
    if body.len() > MAX_BODY {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Full::new(Bytes::from_static(b"payload too large")))
            .expect("response"));
    }
    let (resp_tx, resp_rx) = oneshot::channel();
    let pending = PendingReq {
        method,
        path,
        query,
        headers_json,
        body: body.to_vec(),
        resp_tx,
    };
    if tx.send(pending).is_err() {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Full::new(Bytes::from_static(b"server closed")))
            .expect("response"));
    }
    match resp_rx.await {
        Ok((status, headers_json, body)) => {
            let mut builder = Response::builder().status(status);
            if let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(&headers_json)
            {
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        builder = builder.header(k, s);
                    }
                }
            }
            Ok(builder
                .body(Full::new(Bytes::from(body)))
                .expect("response"))
        }
        Err(_) => Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::new(Bytes::from_static(b"handler dropped")))
            .expect("response")),
    }
}

/// Blocking accept: `ok\nid\nmethod\npath\nquery\nheaders_json\n`, `shutdown\n`, or `err\n…`.
pub(crate) fn accept(server: i32) -> Vec<u8> {
    if server <= 0 {
        return encode_err("invalid server handle");
    }
    let (incoming, shutdown_flag) = {
        let servers = tables().servers.lock().expect("servers");
        let Some(slot) = servers.get(&(server as u32)) else {
            return encode_err("unknown server handle");
        };
        if slot.shutdown.load(Ordering::Relaxed) {
            return encode_shutdown();
        }
        (slot.incoming.clone(), slot.shutdown.clone())
    };
    let pending = loop {
        if shutdown_flag.load(Ordering::Relaxed) {
            return encode_shutdown();
        }
        let incoming = incoming.lock().expect("incoming");
        match incoming.try_recv() {
            Ok(p) => break p,
            Err(mpsc::TryRecvError::Empty) => {
                drop(incoming);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(mpsc::TryRecvError::Disconnected) => return encode_shutdown(),
        }
    };
    let id = tables().next_req.fetch_add(1, Ordering::Relaxed);
    let live = LiveReq {
        method: pending.method,
        path: pending.path,
        query: pending.query,
        headers_json: pending.headers_json,
        body: pending.body,
        resp_tx: pending.resp_tx,
    };
    let wire = encode_accept_ok(id, &live);
    tables()
        .requests
        .lock()
        .expect("requests")
        .insert(id, live);
    wire
}

pub(crate) fn read_body(req: i32, max_bytes: i32) -> Vec<u8> {
    if req <= 0 {
        return Vec::new();
    }
    let requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get(&(req as u32)) else {
        return Vec::new();
    };
    let max = if max_bytes <= 0 {
        live.body.len()
    } else {
        max_bytes as usize
    };
    live.body.iter().take(max).copied().collect()
}

pub(crate) fn respond(req: i32, status: i32, headers_json: &str, body: Vec<u8>) -> i32 {
    if req <= 0 {
        return 0;
    }
    let Some(live) = tables()
        .requests
        .lock()
        .expect("requests")
        .shift_remove(&(req as u32))
    else {
        return 0;
    };
    let status = if (100..600).contains(&status) {
        status as u16
    } else {
        500
    };
    match live.resp_tx.send((status, headers_json.to_string(), body)) {
        Ok(()) => 1,
        Err(_) => 0,
    }
}

pub(crate) fn shutdown(server: i32) -> i32 {
    if server <= 0 {
        return 0;
    }
    let id = server as u32;
    if let Some(slot) = tables().servers.lock().expect("servers").get(&id) {
        slot.shutdown.store(true, Ordering::Relaxed);
    }
    if let Some(tx) = tables().waits.lock().expect("waits").shift_remove(&id) {
        let _ = tx.send(());
    }
    // Unblock a parked accept by closing the incoming channel: drop is not possible while
    // the hyper loop still holds `tx`. Accept sees shutdown flag on the next iteration.
    1
}

/// Completes when `shutdown` is called for this server (or immediately if already stopped).
pub(crate) fn wait(server: i32) -> Vec<u8> {
    if server <= 0 {
        return encode_err("invalid server handle");
    }
    let id = server as u32;
    let already = tables()
        .servers
        .lock()
        .expect("servers")
        .get(&id)
        .map(|s| s.shutdown.load(Ordering::Relaxed))
        .unwrap_or(true);
    if already {
        return b"ok\n".to_vec();
    }
    let (tx, rx) = mpsc::channel();
    tables().waits.lock().expect("waits").insert(id, tx);
    let _ = rx.recv();
    b"ok\n".to_vec()
}
