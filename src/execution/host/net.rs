//! Native TCP (`TcpClient`) and WebSocket (`WebSocket`) hosts. Wire formats match
//! `runtime/src/hosts/net_sockets.js` so Dream `NetWireReader` stays host-agnostic.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use indexmap::IndexMap;
use tungstenite::protocol::CloseFrame;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

fn tcp_table() -> &'static Mutex<IndexMap<u32, TcpStream>> {
    static T: OnceLock<Mutex<IndexMap<u32, TcpStream>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(IndexMap::new()))
}

type WsStream = WebSocket<MaybeTlsStream<TcpStream>>;

fn ws_table() -> &'static Mutex<IndexMap<u32, WsStream>> {
    static T: OnceLock<Mutex<IndexMap<u32, WsStream>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_TCP: AtomicU32 = AtomicU32::new(1);
static NEXT_WS: AtomicU32 = AtomicU32::new(1);

fn wire_text(line: &str, rest: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(line.len() + 1 + rest.len());
    out.extend_from_slice(line.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(rest.as_bytes());
    out
}

fn wire_bytes(tag: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(tag.len() + 1 + payload.len());
    out.extend_from_slice(tag.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(payload);
    out
}

fn lock_map<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub(crate) fn tcp_connect(host: &str, port: i32, timeout_ms: i32) -> Vec<u8> {
    if !(0..=65535).contains(&port) {
        return wire_text("-1", "invalid port");
    }
    let addr = format!("{host}:{port}");
    let addrs = match addr.to_socket_addrs() {
        Ok(a) => a.collect::<Vec<SocketAddr>>(),
        Err(e) => return wire_text("-1", &e.to_string()),
    };
    if addrs.is_empty() {
        return wire_text("-1", "name resolution failed");
    }
    let stream = if timeout_ms > 0 {
        let wait = Duration::from_millis(timeout_ms as u64);
        let mut last = None;
        let mut ok = None;
        for a in addrs {
            match TcpStream::connect_timeout(&a, wait) {
                Ok(s) => {
                    ok = Some(s);
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        match ok {
            Some(s) => s,
            None => {
                return wire_text("-1", &last.map(|e| e.to_string()).unwrap_or_else(|| "timeout".into()));
            }
        }
    } else {
        match TcpStream::connect(&addrs[..]) {
            Ok(s) => s,
            Err(e) => return wire_text("-1", &e.to_string()),
        }
    };
    let _ = stream.set_nodelay(true);
    let id = NEXT_TCP.fetch_add(1, Ordering::Relaxed);
    lock_map(tcp_table()).insert(id, stream);
    wire_text(&id.to_string(), "")
}

pub(crate) fn tcp_send(handle: i32, data: &[u8]) -> Vec<u8> {
    let mut table = lock_map(tcp_table());
    let Some(stream) = table.get_mut(&(handle as u32)) else {
        return wire_text("-1", "unknown handle");
    };
    match stream.write_all(data) {
        Ok(()) => wire_text(&data.len().to_string(), ""),
        Err(e) => wire_text("-1", &e.to_string()),
    }
}

pub(crate) fn tcp_receive(handle: i32, max_bytes: i32) -> Vec<u8> {
    let cap = (max_bytes.max(1) as usize).min(1 << 20);
    let mut table = lock_map(tcp_table());
    let Some(stream) = table.get_mut(&(handle as u32)) else {
        return wire_bytes("error", b"unknown handle");
    };
    let mut buf = vec![0u8; cap];
    match stream.read(&mut buf) {
        Ok(0) => wire_text("eof", ""),
        Ok(n) => wire_bytes("data", &buf[..n]),
        Err(e) => wire_bytes("error", e.to_string().as_bytes()),
    }
}

pub(crate) fn tcp_close(handle: i32) -> i32 {
    lock_map(tcp_table())
        .shift_remove(&(handle as u32))
        .is_some() as i32
}

pub(crate) fn ws_connect(url: &str, timeout_ms: i32) -> Vec<u8> {
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("ws://") && !lower.starts_with("wss://") {
        return wire_text("-2", "unsupported WebSocket scheme");
    }
    let connect = url.to_string();
    let result = if timeout_ms > 0 {
        let wait = Duration::from_millis(timeout_ms as u64);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(tungstenite::connect(connect.as_str()));
        });
        match rx.recv_timeout(wait) {
            Ok(r) => r,
            Err(_) => return wire_text("-1", "timeout"),
        }
    } else {
        tungstenite::connect(connect.as_str())
    };
    match result {
        Ok((ws, _)) => {
            let id = NEXT_WS.fetch_add(1, Ordering::Relaxed);
            lock_map(ws_table()).insert(id, ws);
            wire_text(&id.to_string(), "")
        }
        Err(e) => wire_text("-1", &e.to_string()),
    }
}

pub(crate) fn ws_send_text(handle: i32, text: &str) -> Vec<u8> {
    ws_send(handle, Message::Text(text.to_string()))
}

pub(crate) fn ws_send_binary(handle: i32, data: &[u8]) -> Vec<u8> {
    ws_send(handle, Message::Binary(data.to_vec()))
}

fn ws_send(handle: i32, msg: Message) -> Vec<u8> {
    let mut table = lock_map(ws_table());
    let Some(ws) = table.get_mut(&(handle as u32)) else {
        return wire_text("0", "unknown handle");
    };
    match ws.send(msg) {
        Ok(()) => wire_text("1", ""),
        Err(e) => wire_text("0", &e.to_string()),
    }
}

pub(crate) fn ws_receive(handle: i32) -> Vec<u8> {
    let mut table = lock_map(ws_table());
    let Some(ws) = table.get_mut(&(handle as u32)) else {
        return wire_bytes("error", b"unknown handle");
    };
    loop {
        match ws.read() {
            Ok(Message::Text(s)) => return wire_text("text", &s),
            Ok(Message::Binary(b)) => return wire_bytes("binary", &b),
            Ok(Message::Close(frame)) => {
                let (code, reason) = match frame {
                    Some(CloseFrame { code, reason }) => (u16::from(code) as i32, reason.into_owned()),
                    None => (1000, String::new()),
                };
                drop(table);
                lock_map(ws_table()).shift_remove(&(handle as u32));
                return wire_text("close", &format!("{code}\n{reason}"));
            }
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
            Err(e) => return wire_bytes("error", e.to_string().as_bytes()),
        }
    }
}

pub(crate) fn ws_close(handle: i32, code: i32, reason: &str) -> i32 {
    let mut table = lock_map(ws_table());
    let Some(mut ws) = table.shift_remove(&(handle as u32)) else {
        return 0;
    };
    drop(table);
    let frame = CloseFrame {
        code: tungstenite::protocol::frame::coding::CloseCode::from(code as u16),
        reason: reason.to_string().into(),
    };
    let _ = ws.close(Some(frame));
    1
}

#[cfg(test)]
mod tests {
    use super::{tcp_close, tcp_connect, tcp_receive, tcp_send};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn tcp_echo_roundtrip() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16];
            let n = s.read(&mut buf).unwrap();
            s.write_all(&buf[..n]).unwrap();
        });
        let wire = tcp_connect("127.0.0.1", addr.port() as i32, 5_000);
        let handle: i32 = std::str::from_utf8(&wire)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(handle > 0, "{:?}", String::from_utf8_lossy(&wire));
        let sent = tcp_send(handle, b"ping");
        assert!(sent.starts_with(b"4\n"), "{:?}", String::from_utf8_lossy(&sent));
        let got = tcp_receive(handle, 16);
        assert_eq!(&got[..5], b"data\n");
        assert_eq!(&got[5..], b"ping");
        tcp_close(handle);
        server.join().unwrap();
    }
}
