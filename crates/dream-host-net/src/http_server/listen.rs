use super::{
    multipart,
    request::{self, Slot},
    response,
    state::{self, Config, Server, error, lock, tables},
    stream, tls, websocket,
};
use crate::socket_operations::{self as operations, Failure, Output, OwnedResource};
use dream_host_abi::wire::{Header, Malformed, Reader, Writer};
use hyper::service::service_fn;
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto,
};
use std::{
    collections::BTreeSet,
    net::SocketAddr,
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
/// Operation codes from the guest's WebAction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Listen = 1,
    Accept = 2,
    Shutdown = 3,
    Wait = 4,
    ReadBody = 5,
    BodyRead = 6,
    Respond = 7,
    StreamBegin = 8,
    StreamWrite = 9,
    StreamEnd = 10,
    Upgrade = 11,
    Form = 12,
    UploadRead = 13,
    UploadSeek = 14,
    Disconnect = 15,
    OidcBegin = 16,
    OidcComplete = 17,
}
const ACTIONS: [Action; 17] = [
    Action::Listen,
    Action::Accept,
    Action::Shutdown,
    Action::Wait,
    Action::ReadBody,
    Action::BodyRead,
    Action::Respond,
    Action::StreamBegin,
    Action::StreamWrite,
    Action::StreamEnd,
    Action::Upgrade,
    Action::Form,
    Action::UploadRead,
    Action::UploadSeek,
    Action::Disconnect,
    Action::OidcBegin,
    Action::OidcComplete,
];
fn malformed(_: Malformed) -> Failure {
    error("EINVAL", "malformed HTTP server operation")
}
pub(crate) fn start(request: Vec<u8>, bytes: Vec<u8>) -> i32 {
    operations::OPERATIONS.start(move |mut cancellation| async move {
        let header = Reader::new(&request).header().map_err(malformed)?;
        let deadline = async {
            if header.timeout_ms == 0 {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep(Duration::from_millis(header.timeout_ms.into())).await;
            }
        };
        if *cancellation.borrow() {
            return Err(error("ECANCELLED", "HTTP operation cancelled"));
        }
        tokio::select! {
            result = execute(header, &request[Header::LEN..], bytes) => result,
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
async fn listen(args: &[u8]) -> Result<Output, Failure> {
    let mut reader = Reader::new(args);
    let host = reader.text().map_err(malformed)?.to_owned();
    if host.is_empty() {
        return Err(error("EINVAL", "invalid HTTP host"));
    }
    let port = u16::try_from(reader.i32().map_err(malformed)?)
        .map_err(|_| error("EINVAL", "invalid HTTP port"))?;
    let certfile = reader.text().map_err(malformed)?;
    let keyfile = reader.text().map_err(malformed)?;
    let config = Config::read(&mut reader)?;
    reader.finish().map_err(malformed)?;
    let tls = tls::acceptor(certfile, keyfile)?;
    let listener = bind(&host, port, config.backlog).await?;
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
                    let Ok((socket, peer)) = accepted else { break; };
                    connections.spawn(serve(socket, peer, tls.clone(), incoming.clone(), server.clone()));
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
        bytes: Writer::new().i32(id).i32(local.port().into()).finish(),
        child: Some(OwnedResource::new(id, close)),
    })
}
async fn serve(
    socket: tokio::net::TcpStream,
    peer: SocketAddr,
    tls: Option<tokio_rustls::TlsAcceptor>,
    incoming: mpsc::Sender<i32>,
    server: Arc<Server>,
) {
    let (_slot, _) = Slot::take(&server.connections);
    let ids = Arc::new(Mutex::new(BTreeSet::new()));
    let connection = request::Connection {
        ids: ids.clone(),
        peer,
        secure: tls.is_some(),
    };
    let mut stopping = server.stop.subscribe();
    let config = &server.config;
    let idle = (!config.timeout_keep_alive.is_zero()).then_some(config.timeout_keep_alive);
    // `Some(true)` is ALPN h2, `Some(false)` any other TLS protocol, `None` plain TCP.
    let (io, alpn_h2): (Box<dyn crate::websocket::Transport>, Option<bool>) = match tls {
        Some(tls) => {
            match tokio::time::timeout(Duration::from_secs(10), tls.accept(socket)).await {
                Ok(Ok(stream)) => {
                    let h2 = stream.get_ref().1.alpn_protocol() == Some(b"h2");
                    (Box::new(stream), Some(h2))
                }
                _ => return,
            }
        }
        None => {
            // Protocol detection reads the first bytes before hyper's header timeout starts.
            if let Some(idle) = idle
                && tokio::time::timeout(idle, socket.readable()).await.is_err()
            {
                return;
            }
            (Box::new(socket), None)
        }
    };
    let service_server = server.clone();
    let service = service_fn(move |request| {
        request::dispatch(
            request,
            incoming.clone(),
            connection.clone(),
            service_server.clone(),
        )
    });
    // Each HTTP/2 stream is one request, so `limit_concurrency` still counts requests.
    // WebSocket upgrades exist only on HTTP/1.1 connections.
    let mut builder = auto::Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .keep_alive(true)
        .max_buf_size(config.max_header_bytes)
        .header_read_timeout(idle);
    builder
        .http2()
        .timer(TokioTimer::new())
        .max_concurrent_streams(config.h2_max_concurrent_streams)
        .max_header_list_size(u32::try_from(config.max_header_bytes).unwrap_or(u32::MAX));
    let builder = match alpn_h2 {
        Some(true) => builder.http2_only(),
        Some(false) => builder.http1_only(),
        None => builder,
    };
    let connection = builder.serve_connection_with_upgrades(TokioIo::new(io), service);
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
/// Fails unless the action takes no arguments beyond its header.
fn bare(args: &[u8]) -> Result<(), Failure> {
    Reader::new(args).finish().map_err(malformed)
}
async fn execute(header: Header, args: &[u8], bytes: Vec<u8>) -> Result<Output, Failure> {
    let action = ACTIONS
        .into_iter()
        .find(|action| *action as u8 == header.action)
        .ok_or_else(|| error("EINVAL", "unknown HTTP server operation"))?;
    let id = header.id;
    match action {
        Action::OidcBegin | Action::OidcComplete => {
            let request = crate::oidc::OidcRequest::read(action == Action::OidcComplete, args)?;
            crate::oidc::execute(&request).await
        }
        Action::Listen => listen(args).await,
        Action::Accept => {
            bare(args)?;
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
        Action::Shutdown => {
            bare(args)?;
            state::server(id)?.stop.send_replace(true);
            Ok(Output::bytes(vec![]))
        }
        Action::Wait => {
            bare(args)?;
            let server = state::server(id)?;
            let mut done = server.done.subscribe();
            if !*done.borrow() {
                let _ = done.changed().await;
            }
            Ok(Output::bytes(vec![]))
        }
        Action::ReadBody => {
            bare(args)?;
            Ok(Output::bytes(request::body(id).await?.to_vec()))
        }
        Action::BodyRead => {
            let mut reader = Reader::new(args);
            let max = usize::try_from(reader.i32().map_err(malformed)?)
                .map_err(|_| error("EINVAL", "invalid read size"))?;
            reader.finish().map_err(malformed)?;
            Ok(Output::bytes(request::read_chunk(id, max).await?.to_vec()))
        }
        Action::Respond => response::send(id, args, bytes),
        Action::StreamBegin => stream::begin(id, args),
        Action::StreamWrite => {
            bare(args)?;
            stream::write(id, bytes).await
        }
        Action::StreamEnd => {
            bare(args)?;
            stream::end(id)
        }
        Action::Upgrade => {
            bare(args)?;
            websocket::upgrade(id).await
        }
        Action::Form => multipart::parse(id, args).await,
        Action::UploadRead => multipart::read(id, args).await,
        Action::UploadSeek => multipart::seek(id, args).await,
        Action::Disconnect => {
            bare(args)?;
            let request = state::request(id)?;
            let mut disconnected = request.disconnected.subscribe();
            if !*disconnected.borrow() {
                let _ = disconnected.changed().await;
            }
            Ok(Output::bytes(vec![]))
        }
    }
}
