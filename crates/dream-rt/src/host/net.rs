use crate::guest;
use indexmap::IndexMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

fn wire(tag: impl std::fmt::Display, payload: &[u8]) -> Vec<u8> {
    let mut out = format!("{tag}\n").into_bytes();
    out.extend_from_slice(payload);
    out
}

fn wire_text(tag: impl std::fmt::Display, message: &str) -> Vec<u8> {
    wire(tag, message.as_bytes())
}

fn tcp_table() -> &'static Mutex<IndexMap<u32, TcpStream>> {
    static H: OnceLock<Mutex<IndexMap<u32, TcpStream>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_TCP: AtomicU32 = AtomicU32::new(1);

fn tcp_connect(host: &str, port: i32, timeout_ms: i32) -> Vec<u8> {
    let stream = if timeout_ms > 0 {
        match (host, port as u16).to_socket_addrs() {
            Ok(mut addrs) => match addrs.next() {
                Some(addr) => {
                    TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms as u64))
                }
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "could not resolve host",
                )),
            },
            Err(e) => Err(e),
        }
    } else {
        TcpStream::connect((host, port as u16))
    };
    match stream {
        Ok(stream) => {
            let _ = stream.set_nodelay(true);
            let id = NEXT_TCP.fetch_add(1, Ordering::Relaxed);
            tcp_table()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id, stream);
            wire_text(id, "")
        }
        Err(e) => wire_text(-1, &e.to_string()),
    }
}

type WsStream = WebSocket<MaybeTlsStream<TcpStream>>;

fn ws_table() -> &'static Mutex<IndexMap<u32, WsStream>> {
    static H: OnceLock<Mutex<IndexMap<u32, WsStream>>> = OnceLock::new();
    H.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_WS: AtomicU32 = AtomicU32::new(1);

fn ws_connect(url: &str, timeout_ms: i32) -> Vec<u8> {
    if !(url.starts_with("ws://") || url.starts_with("wss://")) {
        return wire_text(-2, "unsupported URL scheme (expected ws:// or wss://)");
    }
    let socket = if let Some(host_port) = url.strip_prefix("ws://") {
        let (authority, _) = host_port.split_once('/').unwrap_or((host_port, ""));
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
            None => (authority, 80),
        };
        let stream = if timeout_ms > 0 {
            match (host, port).to_socket_addrs() {
                Ok(mut addrs) => match addrs.next() {
                    Some(addr) => {
                        TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms as u64))
                    }
                    None => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                },
                Err(e) => Err(e),
            }
        } else {
            TcpStream::connect((host, port))
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return wire_text(-1, &e.to_string()),
        };
        let _ = stream.set_nodelay(true);
        match tungstenite::client(url, MaybeTlsStream::Plain(stream)) {
            Ok((socket, _)) => socket,
            Err(e) => return wire_text(-1, &e.to_string()),
        }
    } else {
        match tungstenite::connect(url) {
            Ok((socket, _)) => socket,
            Err(e) => return wire_text(-1, &e.to_string()),
        }
    };
    let id = NEXT_WS.fetch_add(1, Ordering::Relaxed);
    ws_table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(id, socket);
    wire_text(id, "")
}

#[no_mangle]
pub extern "C" fn dream_tcp_connect(host: i32, port: i32, timeout_ms: i32) -> i32 {
    guest::write_bytes(&tcp_connect(&guest::read_string(host), port, timeout_ms))
}

#[no_mangle]
pub extern "C" fn dream_tcp_send(handle: i32, data: i32) -> i32 {
    let data = guest::read_bytes(data);
    let mut table = tcp_table().lock().unwrap_or_else(|e| e.into_inner());
    let w = match table.get_mut(&(handle as u32)) {
        Some(s) => match s.write_all(&data) {
            Ok(()) => wire_text(data.len(), ""),
            Err(e) => wire_text(-1, &e.to_string()),
        },
        None => wire_text(-1, "connection not found"),
    };
    guest::write_bytes(&w)
}

#[no_mangle]
pub extern "C" fn dream_tcp_send_text(handle: i32, text: i32) -> i32 {
    let data = guest::read_string(text).into_bytes();
    let mut table = tcp_table().lock().unwrap_or_else(|e| e.into_inner());
    let w = match table.get_mut(&(handle as u32)) {
        Some(s) => match s.write_all(&data) {
            Ok(()) => wire_text(data.len(), ""),
            Err(e) => wire_text(-1, &e.to_string()),
        },
        None => wire_text(-1, "connection not found"),
    };
    guest::write_bytes(&w)
}

#[no_mangle]
pub extern "C" fn dream_tcp_receive(handle: i32, max_bytes: i32) -> i32 {
    let stream = {
        let mut table = tcp_table().lock().unwrap_or_else(|e| e.into_inner());
        table.get_mut(&(handle as u32)).and_then(|s| s.try_clone().ok())
    };
    let w = match stream {
        None => wire_text("error", "connection not found"),
        Some(mut stream) => {
            let cap = (max_bytes.max(1) as usize).min(1 << 20);
            let mut buf = vec![0u8; cap];
            match stream.read(&mut buf) {
                Ok(0) => wire("eof", &[]),
                Ok(n) => wire("data", &buf[..n]),
                Err(e) => wire_text("error", &e.to_string()),
            }
        }
    };
    guest::write_bytes(&w)
}

#[no_mangle]
pub extern "C" fn dream_tcp_close(handle: i32) -> i32 {
    tcp_table()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .shift_remove(&(handle as u32))
        .map(|s| {
            let _ = s.shutdown(std::net::Shutdown::Both);
            1
        })
        .unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn dream_ws_connect(url: i32, timeout_ms: i32) -> i32 {
    guest::write_bytes(&ws_connect(&guest::read_string(url), timeout_ms))
}

#[no_mangle]
pub extern "C" fn dream_ws_send_text(handle: i32, text: i32) -> i32 {
    let msg = Message::Text(guest::read_string(text));
    let mut table = ws_table().lock().unwrap_or_else(|e| e.into_inner());
    let w = match table.get_mut(&(handle as u32)) {
        Some(s) => match s.send(msg) {
            Ok(()) => wire_text(1, ""),
            Err(e) => wire_text(0, &e.to_string()),
        },
        None => wire_text(0, "connection not found"),
    };
    guest::write_bytes(&w)
}

#[no_mangle]
pub extern "C" fn dream_ws_send_binary(handle: i32, data: i32) -> i32 {
    let msg = Message::Binary(guest::read_bytes(data));
    let mut table = ws_table().lock().unwrap_or_else(|e| e.into_inner());
    let w = match table.get_mut(&(handle as u32)) {
        Some(s) => match s.send(msg) {
            Ok(()) => wire_text(1, ""),
            Err(e) => wire_text(0, &e.to_string()),
        },
        None => wire_text(0, "connection not found"),
    };
    guest::write_bytes(&w)
}

#[no_mangle]
pub extern "C" fn dream_ws_receive(handle: i32) -> i32 {
    let mut table = ws_table().lock().unwrap_or_else(|e| e.into_inner());
    let w = match table.get_mut(&(handle as u32)) {
        Some(socket) => loop {
            match socket.read() {
                Ok(Message::Text(text)) => break wire("text", text.as_bytes()),
                Ok(Message::Binary(data)) => break wire("binary", &data),
                Ok(Message::Close(frame)) => {
                    let (code, reason) = frame
                        .map(|f| (u16::from(f.code), f.reason.to_string()))
                        .unwrap_or((1000, String::new()));
                    break wire(format!("close\n{code}"), reason.as_bytes());
                }
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
                Err(e) => break wire_text("error", &e.to_string()),
            }
        },
        None => wire_text("error", "connection not found"),
    };
    guest::write_bytes(&w)
}

#[no_mangle]
pub extern "C" fn dream_ws_close(handle: i32, code: i32, reason: i32) -> i32 {
    let reason = guest::read_string(reason);
    let mut table = ws_table().lock().unwrap_or_else(|e| e.into_inner());
    match table.shift_remove(&(handle as u32)) {
        Some(mut socket) => {
            let frame = tungstenite::protocol::CloseFrame {
                code: (code as u16).into(),
                reason: reason.into(),
            };
            let _ = socket.close(Some(frame));
            1
        }
        None => 0,
    }
}
