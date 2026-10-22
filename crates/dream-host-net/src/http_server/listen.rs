use super::{
    multipart,
    request::{self, Slot},
    response,
    state::{self, Config, Server, error, lock, tables},
    stream, tls, websocket,
};
use crate::socket_operations::{self as operations, Failure, Output, OwnedResource};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpSocket},
    sync::{Mutex as AsyncMutex, mpsc, watch},
    task::JoinSet,
};

pub(crate) fn shutdown(id: i32) {
    if let Ok(server) = state::server(id) {
        server.stop.send_replace(true);
    }
}

pub(crate) fn close(id: i32) {
    if let Some(server) = lock(tables()).servers.remove(id) {
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
async fn bind(host: &str, port: u16, backlog: u32) -> Result<TcpListener, Failure> {
    let mut last = error("EIO", format!("cannot resolve {host}"));
    for address in tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| error("EIO", e))?
    {
        let socket = if address.is_ipv4() {
            TcpSocket::new_v4()
        } else {
            TcpSocket::new_v6()
        }
        .map_err(|e| error("EIO", e))?;
        #[cfg(not(windows))]
        socket.set_reuseaddr(true).map_err(|e| error("EIO", e))?;
        match socket.bind(address).and_then(|()| socket.listen(backlog)) {
            Ok(listener) => return Ok(listener),
            Err(e) => last = error("EIO", e),
        }
    }
    Err(last)
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
    let settings = &value["config"];
    let config = Config::parse(settings)?;
    let tls = tls::acceptor(
        settings["ssl_certfile"].as_str().unwrap_or(""),
        settings["ssl_keyfile"].as_str().unwrap_or(""),
    )?;
    let listener = bind(host, port, config.backlog).await?;
    let local = listener.local_addr().map_err(|e| error("EIO", e))?;
    let (incoming, receiver) = mpsc::channel(256);
    let (stop, mut stopping) = watch::channel(false);
    let (done, _) = watch::channel(false);
    let server = Arc::new(Server {
        config,
        requests: Mutex::new(BTreeSet::new()),
        websockets: Mutex::new(BTreeSet::new()),
        connections: 0.into(),
        in_flight: 0.into(),
        incoming: AsyncMutex::new(receiver),
        stop,
        done,
    });
    let id = lock(tables())
        .servers
        .insert(server.clone())
        .ok_or_else(|| error("ELIMIT", "too many HTTP servers"))?;
    tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                _ = stopping.changed() => break,
                accepted = listener.accept() => {
                    let Ok((socket, _)) = accepted else { break; };
                    connections.spawn(serve(socket, tls.clone(), incoming.clone(), server.clone()));
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        drop(listener);
        drop(incoming);
        server.stop.send_replace(true);
        let sockets = lock(&server.websockets).iter().copied().collect::<Vec<_>>();
        for socket in sockets {
            tokio::spawn(async move {
                crate::websocket::close_with(socket, 1012, "service restart").await
            });
        }
        // Requests that arrived but were never accepted by the guest get a 503.
        let unaccepted = async {
            let mut receiver = server.incoming.lock().await;
            while let Some(id) = receiver.recv().await {
                state::release(id);
            }
        };
        let drain = async {
            let finished = async { while connections.join_next().await.is_some() {} };
            match server.config.timeout_graceful_shutdown {
                Some(limit) => {
                    if tokio::time::timeout(limit, finished).await.is_err() {
                        connections.abort_all();
                        while connections.join_next().await.is_some() {}
                    }
                }
                None => finished.await,
            }
        };
        tokio::join!(unaccepted, drain);
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
async fn serve(
    socket: tokio::net::TcpStream,
    tls: Option<tokio_rustls::TlsAcceptor>,
    incoming: mpsc::Sender<i32>,
    server: Arc<Server>,
) {
    let (_slot, _) = Slot::take(&server.connections);
    let ids = Arc::new(Mutex::new(BTreeSet::new()));
    let connection_ids = ids.clone();
    let mut stopping = server.stop.subscribe();
    let config = &server.config;
    let io: Box<dyn crate::websocket::Transport> = match tls {
        Some(tls) => {
            match tokio::time::timeout(Duration::from_secs(10), tls.accept(socket)).await {
                Ok(Ok(stream)) => Box::new(stream),
                _ => return,
            }
        }
        None => Box::new(socket),
    };
    let service_server = server.clone();
    let service = service_fn(move |request| {
        request::dispatch(
            request,
            incoming.clone(),
            connection_ids.clone(),
            service_server.clone(),
        )
    });
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .keep_alive(true)
        .max_buf_size(config.max_header_bytes)
        .header_read_timeout((!config.timeout_keep_alive.is_zero()).then_some(config.timeout_keep_alive));
    let connection = builder
        .serve_connection(TokioIo::new(io), service)
        .with_upgrades();
    tokio::pin!(connection);
    if !*stopping.borrow_and_update() {
        tokio::select! {
            _ = connection.as_mut() => {}
            _ = stopping.changed() => {
                connection.as_mut().graceful_shutdown();
                let _ = connection.as_mut().await;
            }
        }
    } else {
        connection.as_mut().graceful_shutdown();
        let _ = connection.as_mut().await;
    }
    let ids = std::mem::take(&mut *lock(&ids));
    for id in ids {
        state::disconnect(id);
    }
}
fn handle(value: &Value) -> Result<i32, Failure> {
    value["id"]
        .as_i64()
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| error("EINVAL", "invalid HTTP handle"))
}
async fn execute(value: Value, bytes: Vec<u8>) -> Result<Output, Failure> {
    let action = value["action"].as_str().unwrap_or("");
    if action == "oidc_begin" || action == "oidc_complete" {
        return crate::oidc::execute(&value).await;
    }
    if action == "listen" {
        return listen(&value).await;
    }
    let id = handle(&value)?;
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
        "read_body" => Ok(Output::bytes(request::body(id).await?.to_vec())),
        "body_read" => {
            let max = value["max_bytes"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or_else(|| error("EINVAL", "invalid read size"))?;
            Ok(Output::bytes(request::read_chunk(id, max).await?.to_vec()))
        }
        "respond" => response::send(id, &value, bytes),
        "stream_begin" => stream::begin(id, &value),
        "stream_write" => stream::write(id, bytes).await,
        "stream_end" => stream::end(id),
        "upgrade" => websocket::upgrade(id).await,
        "form" => multipart::parse(id, &value).await,
        "upload_read" => multipart::read(id, &value).await,
        "upload_seek" => multipart::seek(id, &value).await,
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
