//! Raw-byte IPC over the `dream-ipc` custom scheme.
//!
//! wry's `postMessage` handler only carries strings, so byte traffic rides `fetch` instead: the
//! page POSTs the body, the scheme handler parks the responder, and the Dream side answers with
//! the buffer as the HTTP response body. Page-bound byte events reuse the same mechanism through
//! a long-poll ("pull") request that the host completes whenever it has frames queued.
//!
//! Paths: `/i/<reply id>/<channel>` invoke, `/e/0/<channel>` event, `/p` pull. The channel is
//! percent-encoded by the page bridge.

use std::borrow::Cow;
use std::sync::{Mutex, MutexGuard, OnceLock};

use indexmap::IndexMap;
use percent_encoding::percent_decode_str;
use wry::http::{header, HeaderValue, Method, Request, Response, StatusCode};
use wry::RequestAsyncResponder;

use super::{stage_ipc, IpcKind, IpcMessage};

pub(super) const SCHEME: &str = "dream-ipc";

#[derive(Default)]
struct Parked {
    replies: IndexMap<(u32, i32), RequestAsyncResponder>,
    pulls: IndexMap<u32, RequestAsyncResponder>,
    /// Present once the page has pulled at least once; frames emitted before that have no
    /// page-side listener and are dropped instead of accumulating.
    outbox: IndexMap<u32, Vec<u8>>,
}

/// The scheme handler is `'static` and runs from inside the platform event loop, so parked
/// responders live behind a lock rather than in the thread-local webview registry. Never call
/// `respond` while holding it: a responder may synchronously re-enter WebKit.
fn parked() -> MutexGuard<'static, Parked> {
    static PARKED: OnceLock<Mutex<Parked>> = OnceLock::new();
    PARKED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn response(status: StatusCode, body: Vec<u8>) -> Response<Cow<'static, [u8]>> {
    let mut res = Response::new(Cow::Owned(body));
    *res.status_mut() = status;
    let headers = res.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    // `load_html` pages have an opaque origin, so the scheme has to allow any.
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("*"),
    );
    res
}

fn empty(status: StatusCode) -> Response<Cow<'static, [u8]>> {
    response(status, Vec::new())
}

#[derive(Debug, PartialEq, Eq)]
enum Route {
    Invoke { reply_id: i32, channel: String },
    Event { channel: String },
    Pull,
}

fn route(path: &str) -> Option<Route> {
    let mut parts = path.trim_start_matches('/').splitn(3, '/');
    let kind = parts.next()?;
    let n = parts.next().unwrap_or("0");
    let channel = percent_decode_str(parts.next().unwrap_or(""))
        .decode_utf8_lossy()
        .into_owned();
    match kind {
        "i" => Some(Route::Invoke {
            reply_id: n.parse().ok()?,
            channel,
        }),
        "e" => Some(Route::Event { channel }),
        "p" => Some(Route::Pull),
        _ => None,
    }
}

/// `[u32 LE channel len][channel utf8][u32 LE body len][body]`, repeated per event.
fn push_frame(out: &mut Vec<u8>, channel: &str, body: &[u8]) {
    out.reserve(8 + channel.len() + body.len());
    out.extend_from_slice(&(channel.len() as u32).to_le_bytes());
    out.extend_from_slice(channel.as_bytes());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
}

pub(super) fn handle(id: u32, req: Request<Vec<u8>>, responder: RequestAsyncResponder) {
    if req.method() == Method::OPTIONS {
        responder.respond(empty(StatusCode::NO_CONTENT));
        return;
    }
    let route = route(req.uri().path());
    let body = req.into_body();
    match route {
        Some(Route::Invoke { reply_id, channel }) => {
            let replaced = parked().replies.insert((id, reply_id), responder);
            if let Some(old) = replaced {
                old.respond(empty(StatusCode::CONFLICT));
            }
            stage_ipc(
                id,
                IpcMessage {
                    kind: IpcKind::Invoke,
                    binary: true,
                    reply_id,
                    channel,
                    body,
                },
            );
        }
        Some(Route::Event { channel }) => {
            stage_ipc(
                id,
                IpcMessage {
                    kind: IpcKind::Event,
                    binary: true,
                    reply_id: 0,
                    channel,
                    body,
                },
            );
            responder.respond(empty(StatusCode::NO_CONTENT));
        }
        Some(Route::Pull) => {
            let mut p = parked();
            let queued = std::mem::take(p.outbox.entry(id).or_default());
            if queued.is_empty() {
                let old = p.pulls.insert(id, responder);
                drop(p);
                if let Some(old) = old {
                    old.respond(empty(StatusCode::NO_CONTENT));
                }
            } else {
                drop(p);
                responder.respond(response(StatusCode::OK, queued));
            }
        }
        None => responder.respond(empty(StatusCode::NOT_FOUND)),
    }
}

/// Completes a parked byte invoke. `false` when no fetch is waiting on `reply_id`.
pub(super) fn reply(id: u32, reply_id: i32, body: Vec<u8>) -> bool {
    let parked_reply = parked().replies.swap_remove(&(id, reply_id));
    match parked_reply {
        Some(r) => {
            r.respond(response(StatusCode::OK, body));
            true
        }
        None => false,
    }
}

pub(super) fn reject(id: u32, reply_id: i32, message: &str) -> bool {
    let parked_reply = parked().replies.swap_remove(&(id, reply_id));
    match parked_reply {
        Some(r) => {
            r.respond(response(
                StatusCode::INTERNAL_SERVER_ERROR,
                message.as_bytes().to_vec(),
            ));
            true
        }
        None => false,
    }
}

pub(super) fn emit(id: u32, channel: &str, body: &[u8]) {
    let ready = {
        let mut p = parked();
        let Some(queue) = p.outbox.get_mut(&id) else {
            return;
        };
        push_frame(queue, channel, body);
        match p.pulls.swap_remove(&id) {
            Some(pull) => {
                let frames = p.outbox.get_mut(&id).map(std::mem::take).unwrap_or_default();
                Some((pull, frames))
            }
            None => None,
        }
    };
    if let Some((pull, frames)) = ready {
        pull.respond(response(StatusCode::OK, frames));
    }
}

/// Fails every fetch still parked for `id` (window closed or page navigated away).
pub(super) fn release(id: u32) {
    let (replies, pull) = {
        let mut p = parked();
        let keys: Vec<(u32, i32)> = p.replies.keys().filter(|k| k.0 == id).copied().collect();
        let replies: Vec<RequestAsyncResponder> = keys
            .iter()
            .filter_map(|k| p.replies.shift_remove(k))
            .collect();
        p.outbox.shift_remove(&id);
        (replies, p.pulls.shift_remove(&id))
    };
    for r in replies {
        r.respond(empty(StatusCode::GONE));
    }
    if let Some(pull) = pull {
        pull.respond(empty(StatusCode::GONE));
    }
}

#[cfg(test)]
mod tests {
    use super::{push_frame, route, Route};

    #[test]
    fn routes_decode_kind_id_and_channel() {
        assert_eq!(
            route("/i/42/get%20track%2Fbytes"),
            Some(Route::Invoke {
                reply_id: 42,
                channel: "get track/bytes".to_string()
            })
        );
        assert_eq!(
            route("/e/0/tick"),
            Some(Route::Event {
                channel: "tick".to_string()
            })
        );
        assert_eq!(route("/p"), Some(Route::Pull));
        assert_eq!(route("/i/not-a-number/x"), None);
        assert_eq!(route("/zz/0/x"), None);
    }

    #[test]
    fn frames_are_length_prefixed() {
        let mut out = Vec::new();
        push_frame(&mut out, "ab", &[1, 2, 3]);
        assert_eq!(out, vec![2, 0, 0, 0, b'a', b'b', 3, 0, 0, 0, 1, 2, 3]);
    }
}
