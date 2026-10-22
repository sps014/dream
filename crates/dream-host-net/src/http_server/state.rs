use crate::socket_operations::Failure;
use bytes::Bytes;
use dream_host_abi::handles::HandleTable;
use dream_host_abi::wire::Reader;
use http_body_util::combinators::UnsyncBoxBody;
use hyper::{Response, body::Incoming, upgrade::OnUpgrade};
use std::{
    collections::BTreeSet,
    convert::Infallible,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize},
    },
    time::Duration,
};
use tempfile::SpooledTempFile;
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot, watch};

pub(super) type Body = UnsyncBoxBody<Bytes, Infallible>;

/// hyper rejects a read buffer smaller than this.
const MIN_HEADER_BYTES: u32 = 8192;

/// Transport settings from the guest's `ServerConfig`; zero means "none" as in Uvicorn.
pub(super) struct Config {
    pub limit_concurrency: usize,
    pub backlog: u32,
    pub timeout_keep_alive: Duration,
    pub timeout_graceful_shutdown: Option<Duration>,
    pub max_header_bytes: usize,
    pub websocket: crate::websocket::Limits,
}
impl Config {
    /// Reads the settings the guest's `ServerConfig.write_wire` appends after the TLS paths.
    pub fn read(reader: &mut Reader<'_>) -> Result<Self, Failure> {
        let mut number = |name: &str| {
            let value = reader
                .i32()
                .map_err(|_| error("EINVAL", "malformed server settings"))?;
            u32::try_from(value)
                .map_err(|_| error("EINVAL", format!("invalid server setting {name}")))
        };
        let limit_concurrency = number("limit_concurrency")?;
        let backlog = number("backlog")?;
        let keep_alive = number("timeout_keep_alive_ms")?;
        let graceful = number("timeout_graceful_shutdown_ms")?;
        let max_header_bytes = number("max_header_bytes")?;
        let ws_max_size = number("ws_max_size")?;
        let ws_max_queue = number("ws_max_queue")?;
        let ping_interval = number("ws_ping_interval_ms")?;
        let ping_timeout = number("ws_ping_timeout_ms")?;
        if max_header_bytes < MIN_HEADER_BYTES {
            return Err(error(
                "EINVAL",
                format!("max_header_bytes must be at least {MIN_HEADER_BYTES}"),
            ));
        }
        let millis = |n: u32| Duration::from_millis(n.into());
        let some = |n: u32| (n > 0).then_some(millis(n));
        Ok(Self {
            limit_concurrency: n_usize(limit_concurrency),
            backlog: backlog.max(1),
            timeout_keep_alive: millis(keep_alive),
            timeout_graceful_shutdown: some(graceful),
            max_header_bytes: n_usize(max_header_bytes),
            websocket: crate::websocket::Limits {
                max_size: (ws_max_size > 0).then_some(n_usize(ws_max_size)),
                max_queue: n_usize(ws_max_queue).max(1),
                ping_interval: some(ping_interval),
                ping_timeout: millis(ping_timeout),
            },
        })
    }
}

fn n_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}
pub(super) struct Server {
    pub config: Config,
    pub requests: Mutex<BTreeSet<i32>>,
    pub websockets: Mutex<BTreeSet<i32>>,
    /// Open connections, compared with `limit_concurrency`.
    pub connections: AtomicUsize,
    /// Requests between arrival and response, compared with `limit_concurrency`.
    pub in_flight: AtomicUsize,
    pub incoming: AsyncMutex<mpsc::Receiver<i32>>,
    pub stop: watch::Sender<bool>,
    pub done: watch::Sender<bool>,
}

/// Starlette's request body: read once into a cache, or streamed once.
pub(super) enum BodyState {
    Unread(Incoming),
    Streaming { body: Incoming, pending: Bytes },
    Cached { bytes: Bytes, replayed: usize },
    Consumed,
}

pub(super) struct Request {
    pub head: hyper::http::request::Parts,
    pub upgraded: AtomicBool,
    pub accepted: AtomicBool,
    pub server: Arc<Server>,
    pub connection: Arc<Mutex<BTreeSet<i32>>>,
    pub body: AsyncMutex<BodyState>,
    pub uploads: Mutex<Vec<i32>>,
    pub upgrade: AsyncMutex<Option<OnUpgrade>>,
    pub reply: Mutex<Option<oneshot::Sender<Response<Body>>>>,
    pub stream: Mutex<Option<mpsc::Sender<Bytes>>>,
    pub disconnected: watch::Sender<bool>,
}

pub(super) type Upload = Arc<Mutex<SpooledTempFile>>;

pub(super) struct Tables {
    pub servers: HandleTable<Arc<Server>>,
    pub requests: HandleTable<Arc<Request>>,
    pub uploads: HandleTable<Upload>,
}
pub(super) static TABLES: Mutex<Tables> = Mutex::new(Tables {
    servers: HandleTable::new(),
    requests: HandleTable::new(),
    uploads: HandleTable::new(),
});
pub(super) fn tables() -> &'static Mutex<Tables> {
    &TABLES
}
pub(super) use dream_host_abi::lock;
pub(super) fn error(code: &'static str, message: impl ToString) -> Failure {
    (code, message.to_string())
}
pub(super) fn request(id: i32) -> Result<Arc<Request>, Failure> {
    lock(tables())
        .requests
        .get(id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "HTTP request is closed"))
}
pub(super) fn server(id: i32) -> Result<Arc<Server>, Failure> {
    lock(tables())
        .servers
        .get(id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "HTTP server is closed"))
}
pub(super) fn upload(id: i32) -> Result<Upload, Failure> {
    lock(tables())
        .uploads
        .get(id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "uploaded file is closed"))
}
pub(crate) fn release(id: i32) {
    let (request, uploads) = {
        let mut tables = lock(tables());
        let Some(request) = tables.requests.remove(id) else {
            return;
        };
        let uploads = std::mem::take(&mut *lock(&request.uploads))
            .into_iter()
            .filter_map(|upload| tables.uploads.remove(upload))
            .collect::<Vec<_>>();
        (request, uploads)
    };
    drop(uploads);
    lock(&request.connection).remove(&id);
    lock(&request.server.requests).remove(&id);
    lock(&request.stream).take();
    lock(&request.reply).take();
    request.disconnected.send_replace(true);
}
pub(super) fn disconnect(id: i32) {
    let request = lock(tables()).requests.get(id).cloned();
    if let Some(request) = request {
        if !request.accepted.load(std::sync::atomic::Ordering::Acquire) {
            release(id);
        } else if !request.upgraded.load(std::sync::atomic::Ordering::Acquire) {
            request.disconnected.send_replace(true);
        }
    }
}
