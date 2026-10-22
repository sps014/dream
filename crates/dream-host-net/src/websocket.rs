use crate::socket_operations::{self as operations, Failure, Output, OwnedResource};
use futures_util::{Sink, SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex as AsyncMutex, mpsc, watch},
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Message,
        protocol::{CloseFrame, Role, frame::coding::CloseCode},
    },
};

pub(crate) trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}
type Write = Pin<Box<dyn Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Send>>;
struct Socket {
    read: AsyncMutex<mpsc::Receiver<Result<Value, Failure>>>,
    write: Arc<AsyncMutex<Write>>,
    closed: watch::Sender<bool>,
    reader: tokio::task::AbortHandle,
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
#[derive(Default)]
struct Registry {
    next: i32,
    sockets: BTreeMap<i32, Arc<Socket>>,
}
fn registry() -> &'static Mutex<Registry> {
    static VALUE: OnceLock<Mutex<Registry>> = OnceLock::new();
    VALUE.get_or_init(Mutex::default)
}
use dream_host_abi::lock;
fn error(code: &'static str, message: impl ToString) -> Failure {
    (code, message.to_string())
}
fn register<S: Transport + 'static>(stream: WebSocketStream<S>) -> Result<Output, Failure> {
    let mut registry = lock(registry());
    if registry.next == i32::MAX || registry.sockets.len() >= 1024 {
        return Err(error("ELIMIT", "WebSocket capacity exhausted"));
    }
    registry.next += 1;
    let id = registry.next;
    let (write, mut read) = stream.split();
    let write = Arc::new(AsyncMutex::new(Box::pin(write) as Write));
    let (closed, mut stopped) = watch::channel(false);
    let (sender, receiver) = mpsc::channel(64);
    let writer = write.clone();
    let disconnected = closed.clone();
    let task = tokio::spawn(async move {
        let work = async {
            while let Some(message) = read.next().await {
                let output = match message {
                    Ok(Message::Text(text)) => Ok(json!({"kind":"text","text":text.as_str()})),
                    Ok(Message::Binary(bytes)) => {
                        Ok(json!({"kind":"binary","bytes":bytes.to_vec()}))
                    }
                    Ok(Message::Close(frame)) => {
                        let value = json!({"kind":"close","code":frame.as_ref().map(|f| u16::from(f.code)).unwrap_or(1005),"reason":frame.map(|f| f.reason.to_string()).unwrap_or_default()});
                        let _ = writer.lock().await.flush().await;
                        let _ = sender.send(Ok(value)).await;
                        break;
                    }
                    Ok(Message::Ping(_)) => {
                        if writer.lock().await.flush().await.is_err() {
                            break;
                        }
                        continue;
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        let _ = sender.send(Err(error("EIO", e))).await;
                        break;
                    }
                };
                if sender.send(output).await.is_err() {
                    break;
                }
            }
        };
        tokio::select! { _ = work => {}, _ = stopped.changed() => {} }
        disconnected.send_replace(true);
    });
    registry.sockets.insert(
        id,
        Arc::new(Socket {
            read: AsyncMutex::new(receiver),
            write,
            closed,
            reader: task.abort_handle(),
        }),
    );
    Ok(Output {
        bytes: json!({"id":id}).to_string().into_bytes(),
        child: Some(OwnedResource::new(id, close)),
    })
}
pub(crate) async fn from_upgraded(stream: hyper::upgrade::Upgraded) -> Result<Output, Failure> {
    let io: Box<dyn Transport> = Box::new(hyper_util::rt::TokioIo::new(stream));
    register(WebSocketStream::from_raw_socket(io, Role::Server, None).await)
}
pub(crate) fn disconnected(id: i32) -> Result<watch::Receiver<bool>, Failure> {
    Ok(socket(id)?.closed.subscribe())
}
pub(crate) fn close(id: i32) {
    if let Some(socket) = lock(registry()).sockets.remove(&id) {
        socket.closed.send_replace(true);
        socket.reader.abort();
    }
}
fn socket(id: i32) -> Result<Arc<Socket>, Failure> {
    lock(registry())
        .sockets
        .get(&id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "WebSocket is closed"))
}
pub(crate) fn start(request: String, bytes: Vec<u8>) -> i32 {
    operations::OPERATIONS.start(move |mut cancellation| async move {
        let value: Value = serde_json::from_str(&request).map_err(|e| error("EINVAL", e))?;
        let timeout = value["timeout_ms"]
            .as_u64()
            .filter(|&t| t <= i32::MAX as u64)
            .ok_or_else(|| error("EINVAL", "invalid WebSocket timeout"))?;
        let id = value["id"]
            .as_i64()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(0);
        let deadline = async {
            if timeout == 0 {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep(Duration::from_millis(timeout)).await;
            }
        };
        if *cancellation.borrow() {
            close(id);
            return Err(error("ECANCELLED", "WebSocket operation cancelled"));
        }
        tokio::select! {
            result = execute(value, bytes) => result,
            _ = cancellation.changed() => { close(id); Err(error("ECANCELLED", "WebSocket operation cancelled")) },
            _ = deadline => { close(id); Err(error("ETIMEOUT", "WebSocket deadline exceeded")) },
        }
    })
}
async fn execute(value: Value, bytes: Vec<u8>) -> Result<Output, Failure> {
    let action = value["action"].as_str().unwrap_or("");
    if action == "connect" {
        let url = value["url"]
            .as_str()
            .ok_or_else(|| error("EINVAL", "missing WebSocket URL"))?;
        let parsed = url::Url::parse(url).map_err(|e| error("EINVAL", e))?;
        if !matches!(parsed.scheme(), "ws" | "wss") {
            return Err(error("EINVAL", "expected ws:// or wss:// URL"));
        }
        let host = match parsed
            .host()
            .ok_or_else(|| error("EINVAL", "missing WebSocket host"))?
        {
            url::Host::Domain(name) => name.to_string(),
            url::Host::Ipv4(address) => address.to_string(),
            url::Host::Ipv6(address) => address.to_string(),
        };
        let port = parsed
            .port_or_known_default()
            .ok_or_else(|| error("EINVAL", "missing WebSocket port"))?;
        let tcp = tokio::net::TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| error("EIO", e))?;
        let (stream, _) =
            tokio_tungstenite::client_async_tls(url, Box::new(tcp) as Box<dyn Transport>)
                .await
                .map_err(|e| error("EIO", e))?;
        return register(stream);
    }
    let id = value["id"]
        .as_i64()
        .and_then(|i| i32::try_from(i).ok())
        .ok_or_else(|| error("EINVAL", "invalid WebSocket id"))?;
    let socket = socket(id)?;
    if action == "receive" {
        let mut read = socket
            .read
            .try_lock()
            .map_err(|_| error("EBUSY", "WebSocket receive already pending"))?;
        let message = read
            .recv()
            .await
            .unwrap_or_else(|| Ok(json!({"kind":"close","code":1006,"reason":""})))?;
        return Ok(Output::bytes(message.to_string().into_bytes()));
    }
    let mut closed = socket.closed.subscribe();
    if *closed.borrow() {
        return Err(error("ECLOSED", "WebSocket is closed"));
    }
    let message = match action {
        "send_text" => Message::Text(
            String::from_utf8(bytes)
                .map_err(|e| error("EINVAL", e))?
                .into(),
        ),
        "send_binary" => Message::Binary(bytes.into()),
        "close" => {
            let code = value["code"]
                .as_u64()
                .and_then(|c| u16::try_from(c).ok())
                .ok_or_else(|| error("EINVAL", "invalid WebSocket close code"))?;
            let reason = value["reason"].as_str().unwrap_or("");
            let code = CloseCode::from(code);
            if !code.is_allowed() || reason.len() > 123 {
                return Err(error("EINVAL", "invalid WebSocket close frame"));
            }
            Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            }))
        }
        _ => return Err(error("EINVAL", "unknown WebSocket action")),
    };
    let work = async {
        let mut write = socket
            .write
            .try_lock()
            .map_err(|_| error("EBUSY", "WebSocket send already pending"))?;
        write.send(message).await.map_err(|e| error("EIO", e))?;
        Ok(Output::bytes(vec![]))
    };
    tokio::select! { result = work => result, _ = closed.changed() => Err(error("ECLOSED", "WebSocket is closed")) }
}
