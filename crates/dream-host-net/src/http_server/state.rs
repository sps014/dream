use crate::socket_operations::Failure;
use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use hyper::{Response, body::Incoming, upgrade::OnUpgrade};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    convert::Infallible,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::sync::{Mutex as AsyncMutex, OnceCell, mpsc, oneshot, watch};

pub(super) type Body = UnsyncBoxBody<Bytes, Infallible>;
pub(super) const MAX_BODY: usize = 16 * 1024 * 1024;
pub(super) struct Server {
    pub requests: Arc<Mutex<BTreeSet<i32>>>,
    pub incoming: AsyncMutex<mpsc::Receiver<i32>>,
    pub stop: watch::Sender<bool>,
    pub done: watch::Sender<bool>,
}
pub(super) struct Request {
    pub metadata: Value,
    pub upgraded: std::sync::atomic::AtomicBool,
    pub accepted: std::sync::atomic::AtomicBool,
    pub server_requests: Arc<Mutex<BTreeSet<i32>>>,
    pub connection: Arc<Mutex<BTreeSet<i32>>>,
    pub body: AsyncMutex<Option<Incoming>>,
    pub bytes: OnceCell<Result<Vec<u8>, Failure>>,
    pub upgrade: AsyncMutex<Option<OnUpgrade>>,
    pub reply: Mutex<Option<oneshot::Sender<Response<Body>>>>,
    pub stream: Mutex<Option<mpsc::Sender<Bytes>>>,
    pub disconnected: watch::Sender<bool>,
}
#[derive(Default)]
pub(super) struct Tables {
    pub next: i32,
    pub servers: BTreeMap<i32, Arc<Server>>,
    pub requests: BTreeMap<i32, Arc<Request>>,
}
pub(super) fn tables() -> &'static Mutex<Tables> {
    static TABLES: OnceLock<Mutex<Tables>> = OnceLock::new();
    TABLES.get_or_init(Mutex::default)
}
pub(super) use dream_host_abi::lock;
pub(super) fn error(code: &'static str, message: impl ToString) -> Failure {
    (code, message.to_string())
}
pub(super) fn request(id: i32) -> Result<Arc<Request>, Failure> {
    lock(tables())
        .requests
        .get(&id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "HTTP request is closed"))
}
pub(super) fn server(id: i32) -> Result<Arc<Server>, Failure> {
    lock(tables())
        .servers
        .get(&id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "HTTP server is closed"))
}
pub(super) fn next(tables: &mut Tables) -> Result<i32, Failure> {
    tables.next = tables
        .next
        .checked_add(1)
        .ok_or_else(|| error("ELIMIT", "HTTP handle capacity exhausted"))?;
    Ok(tables.next)
}
pub(crate) fn release(id: i32) {
    let request = lock(tables()).requests.remove(&id);
    if let Some(request) = request {
        lock(&request.connection).remove(&id);
        lock(&request.server_requests).remove(&id);
        lock(&request.stream).take();
        lock(&request.reply).take();
        request.disconnected.send_replace(true);
    }
}
pub(super) fn disconnect(id: i32) {
    let request = lock(tables()).requests.get(&id).cloned();
    if let Some(request) = request {
        if !request.accepted.load(std::sync::atomic::Ordering::Acquire) {
            release(id);
        } else if !request.upgraded.load(std::sync::atomic::Ordering::Acquire) {
            request.disconnected.send_replace(true);
        }
    }
}
