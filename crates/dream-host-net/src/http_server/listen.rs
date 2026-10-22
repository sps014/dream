use super::{
    multipart, request, response,
    state::{self, Server, error, lock, tables},
    stream, tls, websocket,
};
use crate::socket_operations::{self as operations, Failure, Output, OwnedResource};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Mutex as AsyncMutex, mpsc, watch},
};

pub(crate) fn shutdown(id: i32) {
    if let Ok(server) = state::server(id) {
        server.stop.send_replace(true);
    }
}

pub(crate) fn close(id: i32) {
    if let Some(server) = lock(tables()).servers.remove(&id) {
        server.stop.send_replace(true);
    }
}
pub(crate) fn start(request: String, bytes: Vec<u8>) -> i32 {
    operations::OPERATIONS.start(move |mut cancellation| async move {
        let value: Value = serde_json::from_str(&request).map_err(|e| error("EINVAL", e))?;
        let timeout = value["timeout_ms"]
            .as_u64()
            .filter(|&t| t <= i32::MAX as u64)
            .ok_or_else(|| error("EINVAL", "invalid HTTP operation timeout"))?;
        let deadline = async {
            if timeout == 0 {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep(Duration::from_millis(timeout)).await;
            }
        };
        if *cancellation.borrow() {
            return Err(error("ECANCELLED", "HTTP operation cancelled"));
        }
        tokio::select! {
            result = execute(value, bytes) => result,
            _ = cancellation.changed() => Err(error("ECANCELLED", "HTTP operation cancelled")),
            _ = deadline => Err(error("ETIMEOUT", "HTTP operation deadline exceeded")),
        }
    })
}
async fn listen(value: &Value) -> Result<Output, Failure> {
    let host = value["host"]
        .as_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| error("EINVAL", "invalid HTTP host"))?;
    let port = value["port"]
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| error("EINVAL", "invalid HTTP port"))?;
    let tls = tls::acceptor(
        value["cert"].as_str().unwrap_or(""),
        value["key"].as_str().unwrap_or(""),
    )?;
    let listener = TcpListener::bind((host, port))
        .await
        .map_err(|e| error("EIO", e))?;
    let local = listener.local_addr().map_err(|e| error("EIO", e))?;
    let (incoming, receiver) = mpsc::channel(256);
    let (stop, mut stopping) = watch::channel(false);
    let (done, _) = watch::channel(false);
    let server = Arc::new(Server {
        requests: Arc::new(Mutex::new(std::collections::BTreeSet::new())),
        incoming: AsyncMutex::new(receiver),
        stop,
        done,
    });
    let id = {
        let mut tables = lock(tables());
        if tables.servers.len() >= 128 {
            return Err(error("ELIMIT", "HTTP server capacity exhausted"));
        }
        let id = state::next(&mut tables)?;
        tables.servers.insert(id, server.clone());
        id
    };
    tokio::spawn(async move {
        let mut connections = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = stopping.changed() => break,
                accepted = listener.accept() => {
                    let Ok((socket, _)) = accepted else { break; };
                    if connections.len() >= 1024 { drop(socket); continue; }
                    let tls = tls.clone();
                    let incoming = incoming.clone();
                    let server_requests = server.requests.clone();
                    let mut stopping = stopping.clone();
                    connections.spawn(async move {
                        let ids = Arc::new(Mutex::new(std::collections::BTreeSet::new()));
                        let connection_ids = ids.clone();
                        let serve = async move {
                            let io: Box<dyn crate::websocket::Transport> = if let Some(tls) = tls {
                                Box::new(tokio::time::timeout(Duration::from_secs(10), tls.accept(socket)).await.map_err(|e| error("ETIMEOUT", e))?.map_err(|e| error("EIO", e))?)
                            } else { Box::new(socket) };
                            let service = service_fn(move |request| request::dispatch(request, incoming.clone(), connection_ids.clone(), server_requests.clone()));
                            http1::Builder::new().serve_connection(TokioIo::new(io), service).with_upgrades().await.map_err(|e| error("EIO", e))
                        };
                        tokio::select! { _ = serve => {}, _ = stopping.changed() => {} }
                        let ids = std::mem::take(&mut *lock(&ids));
                        for id in ids { state::disconnect(id); }
                    });
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        server.stop.send_replace(true);
        while connections.join_next().await.is_some() {}
        let requests = lock(&server.requests).iter().copied().collect::<Vec<_>>();
        for id in requests {
            if let Ok(request) = state::request(id) {
                request.disconnected.send_replace(true);
            }
        }
        server.done.send_replace(true);
    });
    Ok(Output {
        bytes: json!({"id":id,"port":local.port()})
            .to_string()
            .into_bytes(),
        child: Some(OwnedResource::new(id, close)),
    })
}
async fn execute(value: Value, bytes: Vec<u8>) -> Result<Output, Failure> {
    let action = value["action"].as_str().unwrap_or("");
    if action == "oidc_begin" || action == "oidc_complete" {
        return crate::oidc::execute(&value).await;
    }
    if action == "listen" {
        return listen(&value).await;
    }
    let id = value["id"]
        .as_i64()
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| error("EINVAL", "invalid HTTP handle"))?;
    match action {
        "accept" => {
            let server = state::server(id)?;
            let mut stop = server.stop.subscribe();
            if *stop.borrow() {
                return Err(error("ECLOSED", "HTTP server stopped"));
            }
            let mut receiver = server
                .incoming
                .try_lock()
                .map_err(|_| error("EBUSY", "HTTP accept already pending"))?;
            tokio::select! { result = receiver.recv() => request::accepted(result.ok_or_else(|| error("ECLOSED", "HTTP server stopped"))?), _ = stop.changed() => Err(error("ECLOSED", "HTTP server stopped")) }
        }
        "shutdown" => {
            state::server(id)?.stop.send_replace(true);
            Ok(Output::bytes(vec![]))
        }
        "wait" => {
            let server = state::server(id)?;
            let mut done = server.done.subscribe();
            if !*done.borrow() {
                let _ = done.changed().await;
            }
            Ok(Output::bytes(vec![]))
        }
        "read_body" => Ok(Output::bytes(request::body(id).await?)),
        "respond" => response::send(id, &value, bytes),
        "stream_begin" => stream::begin(id, &value),
        "stream_write" => stream::write(id, bytes).await,
        "stream_end" => stream::end(id),
        "upgrade" => websocket::upgrade(id).await,
        "multipart" => multipart::parse(id).await,
        "disconnect" => {
            let request = state::request(id)?;
            let mut disconnected = request.disconnected.subscribe();
            if !*disconnected.borrow() {
                let _ = disconnected.changed().await;
            }
            Ok(Output::bytes(vec![]))
        }
        _ => Err(error("EINVAL", "unknown HTTP server operation")),
    }
}
