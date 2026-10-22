use crate::socket_operations::{self as operations, Failure, Output};
use dream_host_abi::handles::HandleTable;
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        TcpListener, TcpStream, UdpSocket,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{Mutex as AsyncMutex, watch},
};

/// Holds the largest UDP payload over IPv4 or IPv6 without jumbograms.
const MAX_DATAGRAM: usize = 65_536;

enum Socket {
    Tcp {
        read: AsyncMutex<OwnedReadHalf>,
        write: AsyncMutex<Option<OwnedWriteHalf>>,
    },
    Listener(TcpListener),
    Udp {
        socket: UdpSocket,
        receiving: AsyncMutex<()>,
    },
}
struct Resource {
    socket: Socket,
    closed: watch::Sender<bool>,
}
static SOCKETS: Mutex<HandleTable<Arc<Resource>>> = Mutex::new(HandleTable::new());
use dream_host_abi::lock;
fn error(code: &'static str, message: impl ToString) -> Failure {
    (code, message.to_string())
}
fn io(error: std::io::Error) -> Failure {
    crate::sockets::error("EIO", error)
}
fn resource(id: i32) -> Result<Arc<Resource>, Failure> {
    lock(&SOCKETS)
        .get(id)
        .cloned()
        .ok_or_else(|| error("ECLOSED", "socket is closed"))
}
pub(crate) fn close(id: i32) {
    if let Some(socket) = lock(&SOCKETS).remove(id) {
        socket.closed.send_replace(true);
    }
}
fn register(
    socket: Socket,
    local: SocketAddr,
    peer: Option<SocketAddr>,
) -> Result<Output, Failure> {
    let (closed, _) = watch::channel(false);
    let id = lock(&SOCKETS)
        .insert(Arc::new(Resource { socket, closed }))
        .ok_or_else(|| error("ELIMIT", "socket capacity exhausted"))?;
    let bytes = json!({"id":id,"local_host":local.ip().to_string(),"local_port":local.port(),"peer_host":peer.map(|p| p.ip().to_string()),"peer_port":peer.map(|p| p.port())}).to_string().into_bytes();
    Ok(Output {
        bytes,
        child: Some(operations::OwnedResource::new(id, close)),
    })
}
fn register_tcp(socket: TcpStream) -> Result<Output, Failure> {
    let local = socket.local_addr().map_err(io)?;
    let peer = socket.peer_addr().map_err(io)?;
    let (read, write) = socket.into_split();
    register(
        Socket::Tcp {
            read: AsyncMutex::new(read),
            write: AsyncMutex::new(Some(write)),
        },
        local,
        Some(peer),
    )
}
fn endpoint(value: &Value) -> Result<(&str, u16), Failure> {
    let host = value["host"]
        .as_str()
        .filter(|h| !h.is_empty() && !h.contains('\0'))
        .ok_or_else(|| error("EINVAL", "invalid socket host"))?;
    let port = value["port"]
        .as_u64()
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| error("EINVAL", "port must be between 0 and 65535"))?;
    Ok((host, port))
}

pub(crate) fn start(request: String, bytes: Vec<u8>) -> i32 {
    operations::OPERATIONS.start(move |mut cancellation| async move {
        let value: Value = serde_json::from_str(&request)
            .map_err(|_| error("EINVAL", "invalid socket request"))?;
        let timeout = value["timeout_ms"]
            .as_u64()
            .filter(|&v| v <= i32::MAX as u64)
            .ok_or_else(|| error("EINVAL", "invalid socket timeout"))?;
        if *cancellation.borrow() {
            return Err(error("ECANCELLED", "socket operation cancelled"));
        }
        let deadline = async {
            if timeout == 0 {
                std::future::pending::<()>().await;
            } else {
                tokio::time::sleep(Duration::from_millis(timeout)).await;
            }
        };
        tokio::select! {
            result = execute(value, bytes) => result,
            _ = cancellation.changed() => Err(error("ECANCELLED", "socket operation cancelled")),
            _ = deadline => Err(error("ETIMEOUT", "socket operation deadline exceeded")),
        }
    })
}
async fn execute(value: Value, bytes: Vec<u8>) -> Result<Output, Failure> {
    let action = value["action"]
        .as_str()
        .ok_or_else(|| error("EINVAL", "missing socket action"))?;
    match action {
        "connect" => return register_tcp(TcpStream::connect(endpoint(&value)?).await.map_err(io)?),
        "listen" => {
            let socket = TcpListener::bind(endpoint(&value)?).await.map_err(io)?;
            let local = socket.local_addr().map_err(io)?;
            return register(Socket::Listener(socket), local, None);
        }
        "udp_bind" => {
            let socket = UdpSocket::bind(endpoint(&value)?).await.map_err(io)?;
            let local = socket.local_addr().map_err(io)?;
            return register(
                Socket::Udp {
                    socket,
                    receiving: AsyncMutex::new(()),
                },
                local,
                None,
            );
        }
        _ => {}
    }
    let id = value["id"]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| error("EINVAL", "invalid socket id"))?;
    let socket = resource(id)?;
    let mut closed = socket.closed.subscribe();
    if *closed.borrow() {
        return Err(error("ECLOSED", "socket is closed"));
    }
    tokio::select! {
        result = operate(&socket.socket, &value, bytes) => result,
        _ = closed.changed() => Err(error("ECLOSED", "socket is closed")),
    }
}
async fn operate(socket: &Socket, value: &Value, bytes: Vec<u8>) -> Result<Output, Failure> {
    match (socket, value["action"].as_str()) {
        (Socket::Listener(listener), Some("accept")) => {
            register_tcp(listener.accept().await.map_err(io)?.0)
        }
        (Socket::Tcp { read, .. }, Some("read")) => {
            let count = value["count"]
                .as_u64()
                .filter(|&v| v <= 16 * 1024 * 1024)
                .ok_or_else(|| error("EINVAL", "invalid socket read size"))?
                as usize;
            let mut bytes = vec![0; count];
            if count > 0 {
                let mut reader = read
                    .try_lock()
                    .map_err(|_| error("EBUSY", "socket already has a pending read"))?;
                let count = reader.read(&mut bytes).await.map_err(io)?;
                bytes.truncate(count);
            }
            Ok(Output::bytes(bytes))
        }
        (Socket::Tcp { write, .. }, Some("write")) => {
            if bytes.len() > 16 * 1024 * 1024 {
                return Err(error("EINVAL", "socket writes may not exceed 16 MiB"));
            }
            let mut writer = write
                .try_lock()
                .map_err(|_| error("EBUSY", "socket already has a pending write"))?;
            let writer = writer
                .as_mut()
                .ok_or_else(|| error("ECLOSED", "socket write side is shut down"))?;
            let count = if bytes.is_empty() {
                0
            } else {
                writer.write(&bytes).await.map_err(io)?
            };
            Ok(Output::bytes(count.to_string().into_bytes()))
        }
        (Socket::Udp { socket, .. }, Some("udp_send")) => {
            if bytes.len() > 65507 {
                return Err(error("EINVAL", "datagram exceeds portable UDP size limit"));
            }
            let sent = socket.send_to(&bytes, endpoint(value)?).await.map_err(io)?;
            Ok(Output::bytes(sent.to_string().into_bytes()))
        }
        (Socket::Udp { socket, receiving }, Some("udp_receive")) => {
            let count = value["count"]
                .as_u64()
                .filter(|&v| v <= 65535)
                .ok_or_else(|| error("EINVAL", "invalid datagram read size"))?
                as usize;
            let _receiving = receiving
                .try_lock()
                .map_err(|_| error("EBUSY", "datagram receive is already pending"))?;
            // Windows fails recv_from with WSAEMSGSIZE instead of truncating, so receive into a
            // buffer that fits any datagram and truncate to the requested size here.
            let mut bytes = vec![0; MAX_DATAGRAM];
            let (received, peer) = socket.recv_from(&mut bytes).await.map_err(io)?;
            bytes.truncate(received.min(count));
            Ok(Output::bytes(json!({"host":peer.ip().to_string(),"port":peer.port(),"bytes":hex::encode(bytes),"truncated":received > count}).to_string().into_bytes()))
        }
        _ => Err(error("EINVAL", "operation does not match socket kind")),
    }
}
pub(crate) fn control(id: i32, request: &str) -> String {
    let result = (|| {
        let value: Value =
            serde_json::from_str(request).map_err(|_| error("EINVAL", "invalid socket option"))?;
        let socket = resource(id)?;
        match (&socket.socket, value["action"].as_str()) {
            (Socket::Tcp { write, .. }, Some("shutdown_write")) => {
                write
                    .try_lock()
                    .map_err(|_| error("EBUSY", "write is pending"))?
                    .take();
            }
            (Socket::Tcp { read, .. }, Some("nodelay")) => {
                read.try_lock()
                    .map_err(|_| error("EBUSY", "read is pending"))?
                    .as_ref()
                    .set_nodelay(
                        value["value"]
                            .as_bool()
                            .ok_or_else(|| error("EINVAL", "expected boolean option"))?,
                    )
                    .map_err(io)?;
            }
            (Socket::Udp { socket, .. }, Some("broadcast")) => socket
                .set_broadcast(
                    value["value"]
                        .as_bool()
                        .ok_or_else(|| error("EINVAL", "expected boolean option"))?,
                )
                .map_err(io)?,
            (Socket::Udp { socket, .. }, Some("ttl")) => socket
                .set_ttl(
                    value["value"]
                        .as_u64()
                        .filter(|&v| v > 0 && v <= 255)
                        .ok_or_else(|| error("EINVAL", "TTL must be between 1 and 255"))?
                        as u32,
                )
                .map_err(io)?,
            _ => return Err(error("EINVAL", "unsupported socket option")),
        }
        Ok(())
    })();
    match result {
        Ok(()) => "{}".into(),
        Err((code, message)) => json!({"code":code,"message":message}).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn start(action: &str, extra: Value, bytes: &[u8]) -> i32 {
        let mut request = json!({"action":action,"timeout_ms":1000});
        for (key, value) in extra.as_object().unwrap() {
            request[key] = value.clone();
        }
        super::start(request.to_string(), bytes.to_vec())
    }
    fn complete(id: i32, claim: bool) -> Vec<u8> {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let wire = operations::OPERATIONS.result(id);
            if wire[0] != 0 {
                if claim {
                    operations::OPERATIONS.claim(id);
                }
                operations::OPERATIONS.release(id);
                return wire;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "socket operation timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    fn metadata(wire: &[u8]) -> Value {
        assert_eq!(wire[0], 1, "{}", String::from_utf8_lossy(wire));
        serde_json::from_slice(&wire[1..]).unwrap()
    }
    #[test]
    fn resources_cancel_and_unclaimed_handles_are_reclaimed() {
        let baseline = lock(&SOCKETS).len();
        let listener = metadata(&complete(
            start("listen", json!({"host":"127.0.0.1","port":0}), &[]),
            true,
        ));
        let id = listener["id"].as_i64().unwrap() as i32;
        let pending = start("accept", json!({"id":id}), &[]);
        operations::OPERATIONS.cancel(pending);
        assert_eq!(error_code(&complete(pending, false)), "ECANCELLED");
        close(id);
        let unclaimed = start("listen", json!({"host":"127.0.0.1","port":0}), &[]);
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while operations::OPERATIONS.result(unclaimed)[0] == 0 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        operations::OPERATIONS.release(unclaimed);
        assert_eq!(lock(&SOCKETS).len(), baseline);
    }
    fn error_code(wire: &[u8]) -> String {
        assert_eq!(wire[0], 2);
        dream_host_abi::wire::Reader::new(&wire[1..])
            .text()
            .unwrap()
            .to_owned()
    }
}
