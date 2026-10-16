use super::{
    response,
    state::{self, Body, MAX_BODY, Request, error, lock, tables},
};
use crate::socket_operations::{Failure, Output, OwnedResource};
use http_body_util::{BodyExt, Limited};
use hyper::{Request as HttpRequest, Response, body::Incoming};
use serde_json::json;
use std::{convert::Infallible, sync::Arc};
use tokio::sync::{Mutex, OnceCell, mpsc, oneshot, watch};

pub(super) async fn dispatch(
    mut input: HttpRequest<Incoming>,
    incoming: mpsc::Sender<i32>,
    connection: Arc<std::sync::Mutex<std::collections::BTreeSet<i32>>>,
    server_requests: Arc<std::sync::Mutex<std::collections::BTreeSet<i32>>>,
) -> Result<Response<Body>, Infallible> {
    let (reply, result) = oneshot::channel();
    let (disconnected, _) = watch::channel(false);
    let metadata = json!({"method":input.method().as_str(),"path":input.uri().path(),"query":input.uri().query().unwrap_or(""),"headers":input.headers().iter().map(|(k,v)| [k.as_str().to_string(),String::from_utf8_lossy(v.as_bytes()).to_string()]).collect::<Vec<_>>()});
    let upgrade = hyper::upgrade::on(&mut input);
    let request = Arc::new(Request {
        metadata,
        upgraded: std::sync::atomic::AtomicBool::new(false),
        accepted: std::sync::atomic::AtomicBool::new(false),
        connection: connection.clone(),
        server_requests: server_requests.clone(),
        body: Mutex::new(Some(input.into_body())),
        bytes: OnceCell::new(),
        upgrade: Mutex::new(Some(upgrade)),
        reply: std::sync::Mutex::new(Some(reply)),
        stream: std::sync::Mutex::new(None),
        disconnected,
    });
    let id = {
        let mut tables = lock(tables());
        if tables.requests.len() >= 4096 {
            return Ok(response::simple(503, b"HTTP request capacity exhausted"));
        }
        match state::next(&mut tables) {
            Ok(id) => {
                tables.requests.insert(id, request.clone());
                id
            }
            Err(_) => return Ok(response::simple(503, b"HTTP request capacity exhausted")),
        }
    };
    lock(&connection).insert(id);
    lock(&server_requests).insert(id);
    let mut disconnected = request.disconnected.subscribe();
    if incoming.send(id).await.is_err() {
        state::release(id);
        return Ok(response::simple(503, b"server stopped"));
    }
    // Releasing the Dream request after sending its response also signals disconnect.
    // A queued response must win when both notifications are ready.
    let response = tokio::select! {
        biased;
        result = result => result.unwrap_or_else(|_| response::simple(500, b"handler abandoned request")),
        _ = disconnected.changed() => response::simple(503, b"request cancelled"),
    };
    Ok(super::stream::guard(response, request))
}
pub(super) fn accepted(id: i32) -> Result<Output, Failure> {
    let request = {
        let tables = lock(tables());
        let request = tables
            .requests
            .get(&id)
            .cloned()
            .ok_or_else(|| error("ECLOSED", "HTTP client disconnected"))?;
        request
            .accepted
            .store(true, std::sync::atomic::Ordering::Release);
        request
    };
    let mut metadata = request.metadata.clone();
    metadata["id"] = json!(id);
    Ok(Output {
        bytes: metadata.to_string().into_bytes(),
        child: Some(OwnedResource::new(id, state::release)),
    })
}
pub(super) async fn body(id: i32) -> Result<Vec<u8>, Failure> {
    let request = state::request(id)?;
    request
        .bytes
        .get_or_init(|| async {
            let body = request
                .body
                .lock()
                .await
                .take()
                .ok_or_else(|| error("ECLOSED", "HTTP body already consumed"))?;
            Limited::new(body, MAX_BODY)
                .collect()
                .await
                .map(|b| b.to_bytes().to_vec())
                .map_err(|e| error("ELIMIT", e))
        })
        .await
        .clone()
}
