use super::{
    response,
    state::{self, error, lock},
};
use crate::socket_operations::{Failure, Output};
use base64::Engine as _;
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, UPGRADE};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

pub(super) async fn upgrade(id: i32) -> Result<Output, Failure> {
    let request = state::request(id)?;
    let header = |name: &str| {
        request
            .head
            .headers
            .get_all(name)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>()
            .join(",")
    };
    let key = header("sec-websocket-key");
    let decoded = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&key)
        .map_err(|e| error("EINVAL", e))?;
    if request.head.method != hyper::Method::GET
        || header("sec-websocket-version") != "13"
        || !header("upgrade").eq_ignore_ascii_case("websocket")
        || !header("connection")
            .split(',')
            .any(|v| v.trim().eq_ignore_ascii_case("upgrade"))
        || base64::engine::general_purpose::STANDARD
            .decode(&key)
            .map_or(true, |bytes| bytes.len() != 16)
    {
        return Err(error("EINVAL", "invalid WebSocket upgrade request"));
    }
    let mut response = response::simple(101, b"");
    response.headers_mut().insert(
        UPGRADE,
        "websocket".parse().map_err(|e| error("EINVAL", e))?,
    );
    response.headers_mut().insert(
        CONNECTION,
        "Upgrade".parse().map_err(|e| error("EINVAL", e))?,
    );
    response.headers_mut().insert(
        SEC_WEBSOCKET_ACCEPT,
        derive_accept_key(decoded.as_bytes())
            .parse()
            .map_err(|e| error("EINVAL", e))?,
    );
    let on_upgrade = request
        .upgrade
        .lock()
        .await
        .take()
        .ok_or_else(|| error("ECLOSED", "HTTP upgrade already consumed"))?;
    let reply = lock(&request.reply)
        .take()
        .ok_or_else(|| error("ECLOSED", "HTTP response already started"))?;
    request
        .upgraded
        .store(true, std::sync::atomic::Ordering::Release);
    reply
        .send(response)
        .map_err(|_| error("ECLOSED", "HTTP client disconnected"))?;
    let upgraded = on_upgrade.await.map_err(|e| error("EIO", e))?;
    let output =
        crate::websocket::from_upgraded(upgraded, &request.server.config.websocket).await?;
    let id = dream_host_abi::wire::Reader::new(&output.bytes)
        .i32()
        .map_err(|_| error("EIO", "invalid WebSocket handle"))?;
    let mut closed = crate::websocket::disconnected(id)?;
    lock(&request.server.websockets).insert(id);
    tokio::spawn(async move {
        if !*closed.borrow() {
            let _ = closed.changed().await;
        }
        lock(&request.server.websockets).remove(&id);
        request.disconnected.send_replace(true);
    });
    Ok(output)
}
