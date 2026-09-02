//! Native HTTP server host (`system.webapi` / `WebApp`): hyper + tokio on a dedicated runtime
//! thread. The Dream cooperative loop accepts requests via `@async_host` and writes responses
//! through a handle table. Streaming, WebSocket upgrade, and optional rustls share this table.

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use indexmap::IndexMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Role;
use tungstenite::Message;

const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_CHUNK: usize = 64 * 1024;

type BoxBody = UnsyncBoxBody<Bytes, Infallible>;

struct PendingReq {
    method: String,
    path: String,
    query: String,
    headers_json: String,
    body: Vec<u8>,
    on_upgrade: Option<hyper::upgrade::OnUpgrade>,
    reply_tx: oneshot::Sender<ReplyMsg>,
}

enum ReplyMsg {
    Complete {
        status: u16,
        headers: String,
        body: Vec<u8>,
    },
    Stream {
        status: u16,
        headers: String,
        rx: tokio::sync::mpsc::Receiver<Bytes>,
    },
    Switch {
        headers: String,
        on_upgrade: hyper::upgrade::OnUpgrade,
        cmd_rx: tokio::sync::mpsc::UnboundedReceiver<WsCmd>,
    },
}

enum LiveState {
    Pending(oneshot::Sender<ReplyMsg>),
    Streaming(tokio::sync::mpsc::Sender<Bytes>),
    Done,
}

struct ParsedMultipart {
    fields: IndexMap<String, String>,
    files: IndexMap<String, (String, String, Vec<u8>)>,
}

struct LiveReq {
    method: String,
    path: String,
    query: String,
    headers_json: String,
    body: Vec<u8>,
    on_upgrade: Option<hyper::upgrade::OnUpgrade>,
    state: LiveState,
    multipart: Option<ParsedMultipart>,
}

enum WsCmd {
    Send(Message, mpsc::Sender<Vec<u8>>),
    Recv(mpsc::Sender<Vec<u8>>),
    Close(i32, String),
}

struct ServerSlot {
    incoming: std::sync::Arc<Mutex<mpsc::Receiver<PendingReq>>>,
    shutdown: std::sync::Arc<AtomicBool>,
}

struct Tables {
    next_server: AtomicU32,
    next_req: AtomicU32,
    next_ws: AtomicU32,
    servers: Mutex<IndexMap<u32, ServerSlot>>,
    requests: Mutex<IndexMap<u32, LiveReq>>,
    waits: Mutex<IndexMap<u32, mpsc::Sender<()>>>,
    websockets: Mutex<IndexMap<u32, tokio::sync::mpsc::UnboundedSender<WsCmd>>>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| Tables {
        next_server: AtomicU32::new(1),
        next_req: AtomicU32::new(1),
        next_ws: AtomicU32::new(1),
        servers: Mutex::new(IndexMap::new()),
        requests: Mutex::new(IndexMap::new()),
        waits: Mutex::new(IndexMap::new()),
        websockets: Mutex::new(IndexMap::new()),
    })
}

fn box_full(b: Bytes) -> BoxBody {
    Full::new(b).map_err(|e| match e {}).boxed_unsync()
}

fn box_empty() -> BoxBody {
    Empty::<Bytes>::new().map_err(|e| match e {}).boxed_unsync()
}

struct ChunkBody {
    rx: tokio::sync::mpsc::Receiver<Bytes>,
}

impl hyper::body::Body for ChunkBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(b)) => Poll::Ready(Some(Ok(Frame::data(b)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
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

fn header_from_json(headers_json: &str, name: &str) -> Option<String> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str(headers_json) else {
        return None;
    };
    let want = name.to_ascii_lowercase();
    for (k, v) in map {
        if k.eq_ignore_ascii_case(&want) {
            return v.as_str().map(str::to_string);
        }
    }
    None
}

fn apply_headers(
    mut builder: hyper::http::response::Builder,
    headers_json: &str,
) -> hyper::http::response::Builder {
    if let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(headers_json)
    {
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                builder = builder.header(k, s);
            }
        }
    }
    builder
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

fn load_tls(cert_path: &str, key_path: &str) -> Result<tokio_rustls::TlsAcceptor, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certs = {
        let file = std::fs::File::open(cert_path).map_err(|e| e.to_string())?;
        let mut reader = std::io::BufReader::new(file);
        rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let key = {
        let file = std::fs::File::open(key_path).map_err(|e| e.to_string())?;
        let mut reader = std::io::BufReader::new(file);
        rustls_pemfile::private_key(&mut reader)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no private key in TLS key file".to_string())?
    };
    let cfg = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(cfg)))
}

/// Bind `host:port` (`port == 0` picks an ephemeral port). Empty cert/key is cleartext.
pub(crate) fn listen(host: &str, port: i32, tls_cert: &str, tls_key: &str) -> Vec<u8> {
    let cert_empty = tls_cert.is_empty();
    let key_empty = tls_key.is_empty();
    if cert_empty != key_empty {
        return encode_err("tls_cert_path and tls_key_path must both be set or both empty");
    }
    let acceptor = if !cert_empty {
        match load_tls(tls_cert, tls_key) {
            Ok(a) => Some(a),
            Err(e) => return encode_err(&e),
        }
    } else {
        None
    };
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
                    let acceptor = acceptor.clone();
                    let _ = std::thread::Builder::new()
                        .name("dream-webapi-conn".into())
                        .spawn(move || serve_connection(stream, tx, shutdown_conn, acceptor));
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
    acceptor: Option<tokio_rustls::TlsAcceptor>,
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
        if let Some(acceptor) = acceptor {
            let tls = match acceptor.accept(tstream).await {
                Ok(s) => s,
                Err(_) => return,
            };
            serve_http(TokioIo::new(tls), tx, shutdown_conn).await;
        } else {
            serve_http(TokioIo::new(tstream), tx, shutdown_conn).await;
        }
    });
}

async fn serve_http<I>(io: TokioIo<I>, tx: mpsc::Sender<PendingReq>, shutdown_conn: Arc<AtomicBool>)
where
    I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let ws_joins: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(Vec::new()));
    let joins_svc = ws_joins.clone();
    let svc = service_fn(move |req: Request<Incoming>| {
        let tx = tx.clone();
        let shutdown_conn = shutdown_conn.clone();
        let joins_svc = joins_svc.clone();
        async move {
            if shutdown_conn.load(Ordering::Relaxed) {
                return Ok::<_, hyper::Error>(
                    Response::builder()
                        .status(StatusCode::SERVICE_UNAVAILABLE)
                        .body(box_full(Bytes::from_static(b"shutting down")))
                        .expect("response"),
                );
            }
            serve_one(req, tx, joins_svc).await
        }
    });
    let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(io, svc)
        .with_upgrades()
        .await;
    let handles = ws_joins
        .lock()
        .expect("ws joins")
        .drain(..)
        .collect::<Vec<_>>();
    for h in handles {
        let _ = h.await;
    }
}

async fn serve_one(
    mut req: Request<Incoming>,
    tx: mpsc::Sender<PendingReq>,
    ws_joins: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
) -> Result<Response<BoxBody>, hyper::Error> {
    let on_upgrade = hyper::upgrade::on(&mut req);
    let method = req.method().as_str().to_string();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().unwrap_or("").to_string();
    let headers_json = headers_to_json(&req);
    let wants_ws = req
        .headers()
        .get(hyper::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|s| s.eq_ignore_ascii_case("websocket"));
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
    let body = if wants_ws || (content_len == 0 && !chunked) {
        Bytes::new()
    } else {
        req.collect().await?.to_bytes()
    };
    if body.len() > MAX_BODY {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(box_full(Bytes::from_static(b"payload too large")))
            .expect("response"));
    }
    let (reply_tx, reply_rx) = oneshot::channel();
    let pending = PendingReq {
        method,
        path,
        query,
        headers_json,
        body: body.to_vec(),
        on_upgrade: Some(on_upgrade),
        reply_tx,
    };
    if tx.send(pending).is_err() {
        return Ok(Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(box_full(Bytes::from_static(b"server closed")))
            .expect("response"));
    }
    match reply_rx.await {
        Ok(ReplyMsg::Complete {
            status,
            headers,
            body,
        }) => {
            let builder = apply_headers(Response::builder().status(status), &headers);
            Ok(builder.body(box_full(Bytes::from(body))).expect("response"))
        }
        Ok(ReplyMsg::Stream {
            status,
            headers,
            rx,
        }) => {
            let builder = apply_headers(Response::builder().status(status), &headers);
            let body = ChunkBody { rx }.map_err(|e| match e {}).boxed_unsync();
            Ok(builder.body(body).expect("response"))
        }
        Ok(ReplyMsg::Switch {
            headers,
            on_upgrade,
            cmd_rx,
        }) => {
            let join = tokio::task::spawn(async move {
                match on_upgrade.await {
                    Ok(upgraded) => {
                        let ws = WebSocketStream::from_raw_socket(
                            TokioIo::new(upgraded),
                            Role::Server,
                            None,
                        )
                        .await;
                        run_ws_loop(ws, cmd_rx).await;
                    }
                    Err(_) => {}
                }
            });
            ws_joins.lock().expect("ws joins").push(join);
            let mut res = Response::builder()
                .status(StatusCode::SWITCHING_PROTOCOLS)
                .header(hyper::header::UPGRADE, "websocket")
                .header(hyper::header::CONNECTION, "upgrade")
                .header("Sec-WebSocket-Accept", {
                    header_from_json(&headers, "sec-websocket-accept").unwrap_or_default()
                })
                .body(box_empty())
                .expect("101 response");
            let _ = res.headers_mut().insert(
                hyper::header::CONNECTION,
                hyper::header::HeaderValue::from_static("upgrade"),
            );
            Ok(res)
        }
        Err(_) => Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(box_full(Bytes::from_static(b"handler dropped")))
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
        on_upgrade: pending.on_upgrade,
        state: LiveState::Pending(pending.reply_tx),
        multipart: None,
    };
    let wire = encode_accept_ok(id, &live);
    tables().requests.lock().expect("requests").insert(id, live);
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

fn take_pending(req: i32) -> Option<(LiveReq, oneshot::Sender<ReplyMsg>)> {
    let mut requests = tables().requests.lock().expect("requests");
    let live = requests.get_mut(&(req as u32))?;
    match std::mem::replace(&mut live.state, LiveState::Done) {
        LiveState::Pending(tx) => {
            let taken = requests.shift_remove(&(req as u32))?;
            Some((taken, tx))
        }
        other => {
            live.state = other;
            None
        }
    }
}

pub(crate) fn respond(req: i32, status: i32, headers_json: &str, body: Vec<u8>) -> i32 {
    if req <= 0 {
        return 0;
    }
    let Some((_, tx)) = take_pending(req) else {
        return 0;
    };
    let status = if (100..600).contains(&status) {
        status as u16
    } else {
        500
    };
    match tx.send(ReplyMsg::Complete {
        status,
        headers: headers_json.to_string(),
        body,
    }) {
        Ok(()) => 1,
        Err(_) => 0,
    }
}

pub(crate) fn start_stream(req: i32, status: i32, headers_json: &str) -> i32 {
    if req <= 0 {
        return 0;
    }
    let (chunk_tx, chunk_rx) = tokio::sync::mpsc::channel::<Bytes>(8);
    let mut requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get_mut(&(req as u32)) else {
        return 0;
    };
    let tx = match std::mem::replace(&mut live.state, LiveState::Done) {
        LiveState::Pending(tx) => tx,
        other => {
            live.state = other;
            return 0;
        }
    };
    let status = if (100..600).contains(&status) {
        status as u16
    } else {
        200
    };
    if tx
        .send(ReplyMsg::Stream {
            status,
            headers: headers_json.to_string(),
            rx: chunk_rx,
        })
        .is_err()
    {
        return 0;
    }
    live.state = LiveState::Streaming(chunk_tx);
    1
}

pub(crate) fn write_chunk(req: i32, data: Vec<u8>) -> i32 {
    if req <= 0 || data.len() > MAX_CHUNK {
        return 0;
    }
    let tx = {
        let requests = tables().requests.lock().expect("requests");
        let Some(live) = requests.get(&(req as u32)) else {
            return 0;
        };
        match &live.state {
            LiveState::Streaming(tx) => tx.clone(),
            _ => return 0,
        }
    };
    match tx.blocking_send(Bytes::from(data)) {
        Ok(()) => 1,
        Err(_) => 0,
    }
}

pub(crate) fn end_stream(req: i32) -> i32 {
    if req <= 0 {
        return 0;
    }
    let mut requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get_mut(&(req as u32)) else {
        return 0;
    };
    match std::mem::replace(&mut live.state, LiveState::Done) {
        LiveState::Streaming(_) => 1,
        other => {
            live.state = other;
            0
        }
    }
}

pub(crate) fn ws_upgrade(req: i32) -> i32 {
    if req <= 0 {
        return 0;
    }
    let mut requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get_mut(&(req as u32)) else {
        return 0;
    };
    let upgrade_hdr = header_from_json(&live.headers_json, "upgrade").unwrap_or_default();
    let conn_hdr = header_from_json(&live.headers_json, "connection").unwrap_or_default();
    let key = header_from_json(&live.headers_json, "sec-websocket-key").unwrap_or_default();
    if !upgrade_hdr.eq_ignore_ascii_case("websocket")
        || !conn_hdr.to_ascii_lowercase().contains("upgrade")
        || key.is_empty()
    {
        return 0;
    }
    let tx = match std::mem::replace(&mut live.state, LiveState::Done) {
        LiveState::Pending(tx) => tx,
        other => {
            live.state = other;
            return 0;
        }
    };
    let Some(on_upgrade) = live.on_upgrade.take() else {
        let _ = tx.send(ReplyMsg::Complete {
            status: 400,
            headers: "{}".into(),
            body: b"upgrade missing".to_vec(),
        });
        return 0;
    };
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let headers = serde_json::json!({
        "Upgrade": "websocket",
        "Connection": "Upgrade",
        "Sec-WebSocket-Accept": accept,
    })
    .to_string();
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<WsCmd>();
    let id = tables().next_ws.fetch_add(1, Ordering::Relaxed);
    if tx
        .send(ReplyMsg::Switch {
            headers,
            on_upgrade,
            cmd_rx,
        })
        .is_err()
    {
        return 0;
    }
    tables()
        .websockets
        .lock()
        .expect("websockets")
        .insert(id, cmd_tx);
    live.state = LiveState::Done;
    requests.shift_remove(&(req as u32));
    id as i32
}

pub(crate) fn ws_send(handle: i32, kind: i32, data: Vec<u8>) -> Vec<u8> {
    let Some(tx) = tables()
        .websockets
        .lock()
        .expect("websockets")
        .get(&(handle as u32))
        .cloned()
    else {
        return b"0\nunknown handle\n".to_vec();
    };
    let msg = if kind == 1 {
        Message::Binary(data)
    } else {
        Message::Text(String::from_utf8_lossy(&data).into_owned())
    };
    let (rtx, rrx) = mpsc::channel();
    if tx.send(WsCmd::Send(msg, rtx)).is_err() {
        return b"0\nclosed\n".to_vec();
    }
    rrx.recv().unwrap_or_else(|_| b"0\nclosed\n".to_vec())
}

pub(crate) fn ws_receive(handle: i32) -> Vec<u8> {
    let Some(tx) = tables()
        .websockets
        .lock()
        .expect("websockets")
        .get(&(handle as u32))
        .cloned()
    else {
        return b"error\nunknown handle".to_vec();
    };
    let (rtx, rrx) = mpsc::channel();
    if tx.send(WsCmd::Recv(rtx)).is_err() {
        return b"error\nclosed".to_vec();
    }
    rrx.recv().unwrap_or_else(|_| b"error\nclosed".to_vec())
}

pub(crate) fn ws_close(handle: i32, code: i32, reason: &str) -> i32 {
    let Some(tx) = tables()
        .websockets
        .lock()
        .expect("websockets")
        .shift_remove(&(handle as u32))
    else {
        return 0;
    };
    let _ = tx.send(WsCmd::Close(code, reason.to_string()));
    1
}

pub(crate) fn parse_multipart(req: i32) -> i32 {
    if req <= 0 {
        return 0;
    }
    let mut requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get_mut(&(req as u32)) else {
        return 0;
    };
    if live.multipart.is_some() {
        return 1;
    }
    let ct = header_from_json(&live.headers_json, "content-type").unwrap_or_default();
    if !ct.to_ascii_lowercase().contains("multipart/form-data") {
        return 0;
    }
    let Ok(boundary) = multer::parse_boundary(&ct) else {
        return 0;
    };
    let body = Bytes::from(live.body.clone());
    let parsed = pollster::block_on(async {
        let stream = futures_util::stream::once(async move { Ok::<_, std::io::Error>(body) });
        let mut mp = multer::Multipart::new(stream, boundary);
        let mut fields = IndexMap::new();
        let mut files = IndexMap::new();
        while let Ok(Some(field)) = mp.next_field().await {
            let name = field.name().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            if let Some(filename) = field.file_name().map(str::to_string) {
                let ctype = field
                    .content_type()
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "application/octet-stream".into());
                match field.bytes().await {
                    Ok(b) => {
                        files.insert(name, (filename, ctype, b.to_vec()));
                    }
                    Err(_) => return None,
                }
            } else {
                match field.text().await {
                    Ok(t) => {
                        fields.insert(name, t);
                    }
                    Err(_) => return None,
                }
            }
        }
        Some(ParsedMultipart { fields, files })
    });
    match parsed {
        Some(p) => {
            live.multipart = Some(p);
            1
        }
        None => 0,
    }
}

pub(crate) fn multipart_field(req: i32, name: &str) -> Vec<u8> {
    if parse_multipart(req) != 1 {
        return b"0".to_vec();
    }
    let requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get(&(req as u32)) else {
        return b"0".to_vec();
    };
    let Some(mp) = &live.multipart else {
        return b"0".to_vec();
    };
    match mp.fields.get(name) {
        Some(v) => format!("1\n{v}").into_bytes(),
        None => b"0".to_vec(),
    }
}

pub(crate) fn multipart_file(req: i32, name: &str) -> Vec<u8> {
    if parse_multipart(req) != 1 {
        return b"0".to_vec();
    }
    let requests = tables().requests.lock().expect("requests");
    let Some(live) = requests.get(&(req as u32)) else {
        return b"0".to_vec();
    };
    let Some(mp) = &live.multipart else {
        return b"0".to_vec();
    };
    let Some((filename, ctype, bytes)) = mp.files.get(name) else {
        return b"0".to_vec();
    };
    let mut out = format!("1\n{filename}\n{ctype}\n").into_bytes();
    out.extend_from_slice(bytes);
    out
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

fn wire_ws_text(tag: &str, rest: &str) -> Vec<u8> {
    format!("{tag}\n{rest}").into_bytes()
}

async fn run_ws_loop(
    mut ws: WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<WsCmd>,
) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            WsCmd::Send(msg, reply) => {
                let out = match ws.send(msg).await {
                    Ok(()) => b"1\n".to_vec(),
                    Err(e) => format!("0\n{e}").into_bytes(),
                };
                let _ = reply.send(out);
            }
            WsCmd::Recv(reply) => loop {
                match ws.next().await {
                    Some(Ok(Message::Text(s))) => {
                        let _ = reply.send(wire_ws_text("text", &s));
                        break;
                    }
                    Some(Ok(Message::Binary(b))) => {
                        let mut out = b"binary\n".to_vec();
                        out.extend_from_slice(&b);
                        let _ = reply.send(out);
                        break;
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let (code, reason) = match frame {
                            Some(f) => (u16::from(f.code) as i32, f.reason.into_owned()),
                            None => (1000, String::new()),
                        };
                        let _ = reply.send(wire_ws_text("close", &format!("{code}\n{reason}")));
                        break;
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {
                        continue;
                    }
                    Some(Err(e)) => {
                        let _ = reply.send(format!("error\n{e}").into_bytes());
                        break;
                    }
                    None => {
                        let _ = reply.send(wire_ws_text("close", "1000\n"));
                        break;
                    }
                }
            },
            WsCmd::Close(code, reason) => {
                let frame = tungstenite::protocol::CloseFrame {
                    code: tungstenite::protocol::frame::coding::CloseCode::from(code as u16),
                    reason: reason.into(),
                };
                let _ = ws.close(Some(frame)).await;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn listen_loopback_ok() {
        let w = super::listen("127.0.0.1", 0, "", "");
        let s = String::from_utf8_lossy(&w);
        assert!(s.starts_with("ok\n"), "listen wire: {}", s);
        let handle: i32 = s.lines().nth(1).unwrap().parse().unwrap();
        super::shutdown(handle);
    }

    #[test]
    fn accept_respond_roundtrip() {
        let w = super::listen("127.0.0.1", 0, "", "");
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

    #[test]
    fn tls_listen_self_signed_roundtrip() {
        let dir = std::env::temp_dir().join("dream_webapi_tls_test");
        let _ = std::fs::create_dir_all(&dir);
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        let rcgen_cert = rcgen::generate_simple_self_signed(["localhost".into()]).unwrap();
        std::fs::write(&cert_path, rcgen_cert.cert.pem()).unwrap();
        std::fs::write(&key_path, rcgen_cert.key_pair.serialize_pem()).unwrap();
        let w = super::listen(
            "127.0.0.1",
            0,
            cert_path.to_str().unwrap(),
            key_path.to_str().unwrap(),
        );
        let s = String::from_utf8_lossy(&w);
        assert!(s.starts_with("ok\n"), "tls listen: {}", s);
        let handle: i32 = s.lines().nth(1).unwrap().parse().unwrap();
        let port: u16 = s.lines().nth(2).unwrap().parse().unwrap();
        let url = format!("https://127.0.0.1:{port}/health");
        let worker = std::thread::spawn(move || {
            reqwest::blocking::Client::builder()
                .danger_accept_invalid_certs(true)
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap()
                .get(&url)
                .send()
                .map(|r| r.status().as_u16())
                .map_err(|e| e.to_string())
        });
        let acc = super::accept(handle);
        let a = String::from_utf8_lossy(&acc);
        assert!(a.starts_with("ok\n"), "tls accept: {}", a);
        let id: i32 = a.lines().nth(1).unwrap().parse().unwrap();
        assert_eq!(super::respond(id, 200, "{}", b"ok".to_vec()), 1);
        let status = worker.join().unwrap().expect("https client");
        assert_eq!(status, 200);
        super::shutdown(handle);
    }

    #[test]
    fn websocket_echo_roundtrip() {
        let w = super::listen("127.0.0.1", 0, "", "");
        let s = String::from_utf8(w).expect("utf8");
        let mut lines = s.lines();
        assert_eq!(lines.next(), Some("ok"));
        let handle: i32 = lines.next().unwrap().parse().unwrap();
        let port: u16 = lines.next().unwrap().parse().unwrap();
        let url = format!("ws://127.0.0.1:{port}/ws");
        let worker = std::thread::spawn(move || {
            tungstenite::connect(url.as_str()).map_err(|e| e.to_string())
        });
        let acc = super::accept(handle);
        let a = String::from_utf8_lossy(&acc);
        assert!(a.starts_with("ok\n"), "accept: {}", a);
        let id: i32 = a.lines().nth(1).unwrap().parse().unwrap();
        let ws_id = super::ws_upgrade(id);
        assert!(ws_id > 0, "upgrade failed");
        let (mut client, _) = worker.join().unwrap().expect("ws client");
        std::thread::sleep(std::time::Duration::from_millis(50));
        client
            .send(tungstenite::Message::Text("ping".into()))
            .unwrap();
        let got = super::ws_receive(ws_id);
        let text = String::from_utf8_lossy(&got);
        assert!(
            text.starts_with("text\n"),
            "ws receive: {:?} bytes={:?}",
            text,
            got
        );
        assert!(text.contains("ping"), "ws receive: {:?}", text);
        super::ws_close(ws_id, 1000, "");
        super::shutdown(handle);
    }

    #[test]
    fn multipart_parse_from_live_req() {
        let w = super::listen("127.0.0.1", 0, "", "");
        let s = String::from_utf8(w).expect("utf8");
        let mut lines = s.lines();
        assert_eq!(lines.next(), Some("ok"));
        let handle: i32 = lines.next().unwrap().parse().unwrap();
        let port: u16 = lines.next().unwrap().parse().unwrap();
        let url = format!("http://127.0.0.1:{port}/upload");
        let body = b"--bnd\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nhi\r\n--bnd\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nabc\r\n--bnd--\r\n".to_vec();
        let worker = std::thread::spawn(move || {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap()
                .post(&url)
                .header("Content-Type", "multipart/form-data; boundary=bnd")
                .body(body)
                .send()
                .map(|r| r.status().as_u16())
                .map_err(|e| e.to_string())
        });
        let acc = super::accept(handle);
        let a = String::from_utf8_lossy(&acc);
        assert!(a.starts_with("ok\n"), "{}", a);
        let id: i32 = a.lines().nth(1).unwrap().parse().unwrap();
        assert_eq!(super::parse_multipart(id), 1);
        let field = String::from_utf8(super::multipart_field(id, "title")).unwrap();
        assert!(field.starts_with("1\n"), "{}", field);
        assert!(field.contains("hi"), "{}", field);
        let file = super::multipart_file(id, "file");
        let file_s = String::from_utf8_lossy(&file);
        assert!(file_s.starts_with("1\n"), "{}", file_s);
        assert!(file_s.contains("a.txt"), "{}", file_s);
        assert_eq!(super::respond(id, 200, "{}", b"ok".to_vec()), 1);
        let status = worker.join().unwrap().expect("client");
        assert_eq!(status, 200);
        super::shutdown(handle);
    }
}
