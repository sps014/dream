use super::{
    response,
    state::{self, Body, BodyState, Request, Server, error, lock, tables},
};
use crate::socket_operations::{Failure, Output, OwnedResource};
use bytes::Bytes;
use dream_host_abi::wire::{VERSION, Writer};
use http_body_util::BodyExt;
use hyper::{Method, Request as HttpRequest, Response, body::Incoming, http::request::Parts};
use std::{
    collections::BTreeSet,
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::sync::{Mutex, mpsc, oneshot, watch};

/// Counts one unit against `limit_concurrency` until dropped.
pub(super) struct Slot<'a>(&'a AtomicUsize);
impl<'a> Slot<'a> {
    pub fn take(counter: &'a AtomicUsize) -> (Self, usize) {
        let count = counter.fetch_add(1, Ordering::AcqRel) + 1;
        (Self(counter), count)
    }
}
impl Drop for Slot<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) async fn dispatch(
    mut input: HttpRequest<Incoming>,
    incoming: mpsc::Sender<i32>,
    connection: Arc<std::sync::Mutex<BTreeSet<i32>>>,
    server: Arc<Server>,
) -> Result<Response<Body>, Infallible> {
    let (_slot, in_flight) = Slot::take(&server.in_flight);
    let limit = server.config.limit_concurrency;
    if limit > 0 && (in_flight > limit || server.connections.load(Ordering::Acquire) > limit) {
        return Ok(response::simple(503, b"Service Unavailable"));
    }
    if *server.stop.borrow() {
        return Ok(response::simple(503, b"server is shutting down"));
    }
    let (reply, result) = oneshot::channel();
    let (disconnected, _) = watch::channel(false);
    let upgrade = hyper::upgrade::on(&mut input);
    let (head, body) = input.into_parts();
    let request = Arc::new(Request {
        head,
        upgraded: false.into(),
        accepted: false.into(),
        connection: connection.clone(),
        server: server.clone(),
        body: Mutex::new(BodyState::Unread(body)),
        uploads: std::sync::Mutex::new(Vec::new()),
        upgrade: Mutex::new(Some(upgrade)),
        reply: std::sync::Mutex::new(Some(reply)),
        stream: std::sync::Mutex::new(None),
        disconnected,
    });
    let Some(id) = lock(tables()).requests.insert(request.clone()) else {
        return Ok(response::simple(503, b"Service Unavailable"));
    };
    lock(&connection).insert(id);
    lock(&server.requests).insert(id);
    let mut disconnected = request.disconnected.subscribe();
    if incoming.send(id).await.is_err() {
        state::release(id);
        return Ok(response::simple(503, b"server is shutting down"));
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
    let request = lock(tables())
        .requests
        .get(id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "HTTP client disconnected"))?;
    request.accepted.store(true, Ordering::Release);
    Ok(Output {
        bytes: record(id, &request.head),
        child: Some(OwnedResource::new(id, state::release)),
    })
}

/// Method codes in the request record; anything else is sent as code 0 followed by its text.
const METHODS: [Method; 9] = [
    Method::GET,
    Method::HEAD,
    Method::POST,
    Method::PUT,
    Method::DELETE,
    Method::PATCH,
    Method::OPTIONS,
    Method::CONNECT,
    Method::TRACE,
];

/// The request record decoded by the guest's `HttpIncoming`: `[version][id][method code]
/// [method text when the code is 0][path][query][header count][name, value]...`.
pub(super) fn record(id: i32, head: &Parts) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.u8(VERSION).i32(id);
    match METHODS.iter().position(|method| *method == head.method) {
        Some(index) => writer.u8(index as u8 + 1),
        None => writer.u8(0).text(head.method.as_str()),
    };
    writer
        .text(head.uri.path())
        .text(head.uri.query().unwrap_or(""))
        .i32(i32::try_from(head.headers.len()).unwrap_or(i32::MAX));
    for (name, value) in &head.headers {
        writer
            .text(name.as_str())
            .text(&String::from_utf8_lossy(value.as_bytes()));
    }
    writer.finish()
}

fn consumed() -> Failure {
    error("ECLOSED", "request body already consumed")
}

pub(super) async fn body(id: i32) -> Result<Bytes, Failure> {
    let request = state::request(id)?;
    let mut state = request.body.lock().await;
    match std::mem::replace(&mut *state, BodyState::Consumed) {
        BodyState::Unread(body) => {
            let bytes = body
                .collect()
                .await
                .map_err(|e| error("EIO", e))?
                .to_bytes();
            *state = BodyState::Cached {
                bytes: bytes.clone(),
                replayed: 0,
            };
            Ok(bytes)
        }
        BodyState::Cached { bytes, replayed } => {
            *state = BodyState::Cached {
                bytes: bytes.clone(),
                replayed,
            };
            Ok(bytes)
        }
        other => {
            *state = other;
            Err(consumed())
        }
    }
}

/// Takes the unread body for a single streaming consumer such as the form parser.
pub(super) async fn take_source(id: i32) -> Result<BodySource, Failure> {
    let request = state::request(id)?;
    let mut state = request.body.lock().await;
    match std::mem::replace(&mut *state, BodyState::Consumed) {
        BodyState::Unread(body) => Ok(BodySource::Stream(body)),
        BodyState::Cached { bytes, replayed } => {
            *state = BodyState::Cached {
                bytes: bytes.clone(),
                replayed,
            };
            Ok(BodySource::Cached(bytes))
        }
        other => {
            *state = other;
            Err(consumed())
        }
    }
}
pub(super) enum BodySource {
    Stream(Incoming),
    Cached(Bytes),
}

/// Returns up to `max` body bytes; an empty result marks the end of the body.
pub(super) async fn read_chunk(id: i32, max: usize) -> Result<Bytes, Failure> {
    let request = state::request(id)?;
    let mut state = request.body.lock().await;
    let max = max.max(1);
    if matches!(*state, BodyState::Unread(_))
        && let BodyState::Unread(body) = std::mem::replace(&mut *state, BodyState::Consumed)
    {
        *state = BodyState::Streaming {
            body,
            pending: Bytes::new(),
        };
    }
    loop {
        match &mut *state {
            BodyState::Streaming { body, pending } => {
                if !pending.is_empty() {
                    let take = pending.len().min(max);
                    return Ok(pending.split_to(take));
                }
                match body.frame().await {
                    Some(Ok(frame)) => {
                        if let Ok(data) = frame.into_data() {
                            *pending = data;
                        }
                    }
                    Some(Err(e)) => {
                        *state = BodyState::Consumed;
                        return Err(error("EIO", e));
                    }
                    None => *state = BodyState::Consumed,
                }
            }
            BodyState::Cached { bytes, replayed } => {
                let start = *replayed;
                let end = bytes.len().min(start + max);
                *replayed = end;
                return Ok(bytes.slice(start..end));
            }
            BodyState::Unread(_) | BodyState::Consumed => return Ok(Bytes::new()),
        }
    }
}
