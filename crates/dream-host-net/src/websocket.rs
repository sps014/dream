use crate::socket_operations::{self as operations, Failure, Output, OwnedResource};
use dream_host_abi::{
    handles::HandleTable,
    wire::{Header, Reader, Writer},
};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Mutex as AsyncMutex, mpsc, watch},
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        self, Message,
        protocol::{CloseFrame, Role, WebSocketConfig, frame::coding::CloseCode},
    },
};

pub(crate) trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Transport for T {}
type Write = Pin<Box<dyn Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Send>>;
struct Socket {
    read: AsyncMutex<mpsc::Receiver<Result<Vec<u8>, Failure>>>,
    write: Arc<AsyncMutex<Write>>,
    closed: watch::Sender<bool>,
    reader: tokio::task::AbortHandle,
}
impl Drop for Socket {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
static SOCKETS: Mutex<HandleTable<Arc<Socket>>> = Mutex::new(HandleTable::new());
use dream_host_abi::lock;
fn error(code: &'static str, message: impl ToString) -> Failure {
    (code, message.to_string())
}
/// Per-socket limits; the server takes them from `ServerConfig`.
pub(crate) struct Limits {
    pub max_size: Option<usize>,
    pub max_queue: usize,
    pub ping_interval: Option<Duration>,
    pub ping_timeout: Duration,
}
impl Limits {
    const CLIENT: Self = Self {
        max_size: None,
        max_queue: 64,
        ping_interval: None,
        ping_timeout: Duration::ZERO,
    };
    fn config(&self) -> Option<WebSocketConfig> {
        self.max_size.map(|size| {
            WebSocketConfig::default()
                .max_message_size(Some(size))
                .max_frame_size(Some(size))
        })
    }
}
fn close_frame(code: u16, reason: &str) -> Message {
    Message::Close(Some(CloseFrame {
        code: CloseCode::from(code),
        reason: reason.into(),
    }))
}
/// Operation codes from the guest's WebSocketAction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Connect = 1,
    Receive = 2,
    SendText = 3,
    SendBinary = 4,
    Close = 5,
}
impl Action {
    fn decode(code: u8) -> Option<Self> {
        [
            Self::Connect,
            Self::Receive,
            Self::SendText,
            Self::SendBinary,
            Self::Close,
        ]
        .into_iter()
        .find(|action| *action as u8 == code)
    }
}
fn malformed(_: dream_host_abi::wire::Malformed) -> Failure {
    error("EINVAL", "malformed WebSocket operation")
}
// Receive completions: [kind] then text (1), bytes (2), or a close code and reason (3).
fn text_event(text: &str) -> Vec<u8> {
    Writer::new().u8(1).text(text).finish()
}
fn binary_event(bytes: &[u8]) -> Vec<u8> {
    Writer::new().u8(2).bytes(bytes).finish()
}
fn closed_event(code: u16, reason: &str) -> Vec<u8> {
    Writer::new().u8(3).i32(code.into()).text(reason).finish()
}
fn register<S: Transport + 'static>(
    stream: WebSocketStream<S>,
    limits: &Limits,
) -> Result<Output, Failure> {
    let (write, mut read) = stream.split();
    let write = Arc::new(AsyncMutex::new(Box::pin(write) as Write));
    let (closed, mut stopped) = watch::channel(false);
    let (sender, receiver) = mpsc::channel(limits.max_queue);
    let writer = write.clone();
    let disconnected = closed.clone();
    let ping_interval = limits.ping_interval;
    let ping_timeout = limits.ping_timeout;
    let task = tokio::spawn(async move {
        let work = async {
            let mut awaiting_pong: Option<Instant> = None;
            let mut last_ping = Instant::now();
            loop {
                let deadline = match (ping_interval, awaiting_pong) {
                    (None, _) => None,
                    (Some(_), Some(sent)) => Some(sent + ping_timeout),
                    (Some(interval), None) => Some(last_ping + interval),
                };
                let timer = async {
                    match deadline {
                        Some(at) => tokio::time::sleep_until(at.into()).await,
                        None => std::future::pending().await,
                    }
                };
                let message = tokio::select! {
                    message = read.next() => message,
                    _ = timer => {
                        if awaiting_pong.is_some() {
                            let reason = "keepalive ping timeout";
                            let _ = writer.lock().await.send(close_frame(1011, reason)).await;
                            let _ = sender.send(Ok(closed_event(1011, reason))).await;
                            break;
                        }
                        let now = Instant::now();
                        last_ping = now;
                        awaiting_pong = Some(now);
                        if writer.lock().await.send(Message::Ping(Default::default())).await.is_err() {
                            break;
                        }
                        continue;
                    }
                };
                let Some(message) = message else {
                    break;
                };
                let output = match message {
                    Ok(Message::Text(text)) => Ok(text_event(text.as_str())),
                    Ok(Message::Binary(bytes)) => Ok(binary_event(&bytes)),
                    Ok(Message::Close(frame)) => {
                        let value = match frame {
                            Some(frame) => closed_event(frame.code.into(), &frame.reason),
                            None => closed_event(1005, ""),
                        };
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
                    Ok(Message::Pong(_)) => {
                        awaiting_pong = None;
                        continue;
                    }
                    Ok(_) => continue,
                    Err(tungstenite::Error::Capacity(_)) => {
                        let reason = "message too big";
                        let _ = writer.lock().await.send(close_frame(1009, reason)).await;
                        let _ = sender.send(Ok(closed_event(1009, reason))).await;
                        break;
                    }
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
    let socket = Arc::new(Socket {
        read: AsyncMutex::new(receiver),
        write,
        closed,
        reader: task.abort_handle(),
    });
    let id = lock(&SOCKETS)
        .insert(socket)
        .ok_or_else(|| error("ELIMIT", "WebSocket capacity exhausted"))?;
    Ok(Output {
        bytes: Writer::new().i32(id).finish(),
        child: Some(OwnedResource::new(id, close)),
    })
}
pub(crate) async fn from_upgraded(
    stream: hyper::upgrade::Upgraded,
    limits: &Limits,
) -> Result<Output, Failure> {
    let io: Box<dyn Transport> = Box::new(hyper_util::rt::TokioIo::new(stream));
    register(
        WebSocketStream::from_raw_socket(io, Role::Server, limits.config()).await,
        limits,
    )
}
pub(crate) fn disconnected(id: i32) -> Result<watch::Receiver<bool>, Failure> {
    Ok(socket(id)?.closed.subscribe())
}
/// Sends a server-initiated close frame; the peer's reply reaches the guest as a close event.
pub(crate) async fn close_with(id: i32, code: u16, reason: &str) {
    let Ok(socket) = socket(id) else {
        return;
    };
    let mut write = socket.write.lock().await;
    let _ = write.send(close_frame(code, reason)).await;
}
pub(crate) fn close(id: i32) {
    if let Some(socket) = lock(&SOCKETS).remove(id) {
        socket.closed.send_replace(true);
        socket.reader.abort();
    }
}
fn socket(id: i32) -> Result<Arc<Socket>, Failure> {
    lock(&SOCKETS)
        .get(id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "WebSocket is closed"))
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
            close(header.id);
            return Err(error("ECANCELLED", "WebSocket operation cancelled"));
        }
        tokio::select! {
            result = execute(header, &request[Header::LEN..], bytes) => result,
            _ = cancellation.changed() => { close(header.id); Err(error("ECANCELLED", "WebSocket operation cancelled")) },
            _ = deadline => { close(header.id); Err(error("ETIMEOUT", "WebSocket deadline exceeded")) },
        }
    })
}
async fn connect(url: &str) -> Result<Output, Failure> {
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
    let (stream, _) = tokio_tungstenite::client_async_tls(url, Box::new(tcp) as Box<dyn Transport>)
        .await
        .map_err(|e| error("EIO", e))?;
    register(stream, &Limits::CLIENT)
}
async fn execute(header: Header, args: &[u8], bytes: Vec<u8>) -> Result<Output, Failure> {
    let action =
        Action::decode(header.action).ok_or_else(|| error("EINVAL", "unknown WebSocket action"))?;
    let mut args = Reader::new(args);
    if action == Action::Connect {
        let url = args.text().map_err(malformed)?;
        args.finish().map_err(malformed)?;
        return connect(url).await;
    }
    let message = match action {
        Action::Close => {
            let code = u16::try_from(args.i32().map_err(malformed)?)
                .map_err(|_| error("EINVAL", "invalid WebSocket close code"))?;
            let reason = args.text().map_err(malformed)?;
            let code = CloseCode::from(code);
            if !code.is_allowed() || reason.len() > 123 {
                return Err(error("EINVAL", "invalid WebSocket close frame"));
            }
            Some(Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            })))
        }
        Action::SendText => Some(Message::Text(
            String::from_utf8(bytes)
                .map_err(|e| error("EINVAL", e))?
                .into(),
        )),
        Action::SendBinary => Some(Message::Binary(bytes.into())),
        Action::Receive | Action::Connect => None,
    };
    args.finish().map_err(malformed)?;
    let socket = socket(header.id)?;
    let Some(message) = message else {
        let mut read = socket
            .read
            .try_lock()
            .map_err(|_| error("EBUSY", "WebSocket receive already pending"))?;
        let message = read
            .recv()
            .await
            .unwrap_or_else(|| Ok(closed_event(1006, "")))?;
        return Ok(Output::bytes(message));
    };
    let mut closed = socket.closed.subscribe();
    if *closed.borrow() {
        return Err(error("ECLOSED", "WebSocket is closed"));
    }
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
