//! Raw TCP (`system.net.TcpClient`) and WebSocket (`system.net.WebSocket`) host functions. Both
//! use blocking I/O on a background-free call (mirroring the HTTP host's blocking `reqwest`
//! client): a connection is opened once and kept in a handle table (like `process.rs`'s spawned
//! children) so later `send`/`receive` calls reuse the same socket.
//!
//! WebSocket uses `tungstenite` with `native-tls`, so both `ws://` and `wss://` are supported.
//! Unknown schemes fail with wire `-2` (see `Cargo.toml`'s `tungstenite` features).
//!
//! Wire formats are shared with the JS host (`runtime/src/hosts/net_sockets.js`) and parsed
//! Dream-side by `NetWireReader` (`crates/dream-stdlib/src/system/net/net_wire_reader.dream`):
//!   connect (`tcpConnect`/`wsConnect`): `"<handle>\n"` on success, `"-1\n<message>"` on failure,
//!     `"-2\n<message>"` for an unsupported scheme.
//!   `tcpSend`/`tcpSendText`: `"<n>\n"` bytes written, or `"-1\n<message>"`.
//!   `tcpReceive`: `"data\n<bytes>"` | `"eof\n"` | `"error\n<message>"`.
//!   `wsSendText`/`wsSendBinary`: `"1\n"` on success, `"0\n<message>"` on failure.
//!   `wsReceive`: `"text\n<utf8>"` | `"binary\n<bytes>"` | `"close\n<code>\n<reason>"` |
//!     `"error\n<message>"`.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use indexmap::IndexMap;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};
use wasmtime::*;

use super::memory::{read_arg_bytes, read_arg_string, resolve_host_future_bytes};

/// Formats a wire payload as `"<tag_line>\n" + payload`, matching the JS host byte-for-byte.
fn wire(tag_line: impl std::fmt::Display, payload: &[u8]) -> Vec<u8> {
    let mut out = format!("{tag_line}\n").into_bytes();
    out.extend_from_slice(payload);
    out
}

fn wire_text(tag_line: impl std::fmt::Display, message: &str) -> Vec<u8> {
    wire(tag_line, message.as_bytes())
}

/* ------------------------------------------------------------------------------------- TCP -- */

fn tcp_handles() -> &'static Mutex<IndexMap<u32, TcpStream>> {
    static HANDLES: OnceLock<Mutex<IndexMap<u32, TcpStream>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_TCP_HANDLE: AtomicU32 = AtomicU32::new(1);

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
            let id = NEXT_TCP_HANDLE.fetch_add(1, Ordering::Relaxed);
            let mut table = tcp_handles().lock().unwrap_or_else(|e| e.into_inner());
            table.insert(id, stream);
            wire_text(id, "")
        }
        Err(e) => wire_text(-1, &e.to_string()),
    }
}

fn tcp_send(handle: i32, data: &[u8]) -> Vec<u8> {
    let mut table = tcp_handles().lock().unwrap_or_else(|e| e.into_inner());
    match table.get_mut(&(handle as u32)) {
        Some(stream) => match stream.write_all(data) {
            Ok(()) => wire_text(data.len(), ""),
            Err(e) => wire_text(-1, &e.to_string()),
        },
        None => wire_text(-1, "connection not found"),
    }
}

fn tcp_receive(handle: i32, max_bytes: i32) -> Vec<u8> {
    let stream = {
        let mut table = tcp_handles().lock().unwrap_or_else(|e| e.into_inner());
        table.get_mut(&(handle as u32)).map(|s| s.try_clone())
    };
    let Some(Ok(mut stream)) = stream else {
        return wire_text("error", "connection not found");
    };
    let cap = (max_bytes.max(1) as usize).min(1 << 20);
    let mut buf = vec![0u8; cap];
    match stream.read(&mut buf) {
        Ok(0) => wire("eof", &[]),
        Ok(n) => wire("data", &buf[..n]),
        Err(e) => wire_text("error", &e.to_string()),
    }
}

fn tcp_close(handle: i32) -> i32 {
    let mut table = tcp_handles().lock().unwrap_or_else(|e| e.into_inner());
    match table.shift_remove(&(handle as u32)) {
        Some(stream) => {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            1
        }
        None => 0,
    }
}

/* ------------------------------------------------------------------------------- WebSocket -- */

type WsStream = WebSocket<MaybeTlsStream<TcpStream>>;

fn ws_handles() -> &'static Mutex<IndexMap<u32, WsStream>> {
    static HANDLES: OnceLock<Mutex<IndexMap<u32, WsStream>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(IndexMap::new()))
}

static NEXT_WS_HANDLE: AtomicU32 = AtomicU32::new(1);

fn ws_connect(url: &str, timeout_ms: i32) -> Vec<u8> {
    if !(url.starts_with("ws://") || url.starts_with("wss://")) {
        return wire_text(-2, "unsupported URL scheme (expected ws:// or wss://)");
    }

    // Plain `ws://` still honors `timeout_ms` on the TCP connect. `wss://` goes through
    // `tungstenite::connect` (TLS handshake included); a non-zero timeout is applied as a
    // read/write deadline on the resulting stream when possible.
    let socket = if let Some(host_port) = url.strip_prefix("ws://") {
        let (authority, _rest) = host_port.split_once('/').unwrap_or((host_port, ""));
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => match p.parse::<u16>() {
                Ok(p) => (h, p),
                Err(_) => (authority, 80),
            },
            None => (authority, 80),
        };

        let stream = if timeout_ms > 0 {
            match (host, port).to_socket_addrs() {
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
            TcpStream::connect((host, port))
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => return wire_text(-1, &e.to_string()),
        };
        let _ = stream.set_nodelay(true);
        match tungstenite::client(url, MaybeTlsStream::Plain(stream)) {
            Ok((socket, _response)) => socket,
            Err(e) => return wire_text(-1, &e.to_string()),
        }
    } else {
        match tungstenite::connect(url) {
            Ok((socket, _response)) => {
                if timeout_ms > 0 {
                    let dur = Duration::from_millis(timeout_ms as u64);
                    if let MaybeTlsStream::NativeTls(s) = socket.get_ref() {
                        let _ = s.get_ref().set_read_timeout(Some(dur));
                        let _ = s.get_ref().set_write_timeout(Some(dur));
                    }
                }
                socket
            }
            Err(e) => return wire_text(-1, &e.to_string()),
        }
    };

    let id = NEXT_WS_HANDLE.fetch_add(1, Ordering::Relaxed);
    let mut table = ws_handles().lock().unwrap_or_else(|e| e.into_inner());
    table.insert(id, socket);
    wire_text(id, "")
}

fn ws_send(handle: i32, message: Message) -> Vec<u8> {
    let mut table = ws_handles().lock().unwrap_or_else(|e| e.into_inner());
    match table.get_mut(&(handle as u32)) {
        Some(socket) => match socket.send(message) {
            Ok(()) => wire_text(1, ""),
            Err(e) => wire_text(0, &e.to_string()),
        },
        None => wire_text(0, "connection not found"),
    }
}

fn ws_receive(handle: i32) -> Vec<u8> {
    let mut table = ws_handles().lock().unwrap_or_else(|e| e.into_inner());
    match table.get_mut(&(handle as u32)) {
        Some(socket) => loop {
            match socket.read() {
                Ok(Message::Text(text)) => break wire("text", text.as_bytes()),
                Ok(Message::Binary(data)) => break wire("binary", &data),
                Ok(Message::Close(frame)) => {
                    let (code, reason) = frame
                        .map(|f| (u16::from(f.code), f.reason.to_string()))
                        .unwrap_or((1000, String::new()));
                    break wire(format_args!("close\n{code}"), reason.as_bytes());
                }
                // Ping/Pong/Frame are transparently handled by tungstenite's `read`; keep polling
                // for the next application-level message.
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                Err(e) => break wire_text("error", &e.to_string()),
            }
        },
        None => wire_text("error", "connection not found"),
    }
}

fn ws_close(handle: i32, code: i32, reason: &str) -> i32 {
    let mut table = ws_handles().lock().unwrap_or_else(|e| e.into_inner());
    match table.shift_remove(&(handle as u32)) {
        Some(mut socket) => {
            let frame = tungstenite::protocol::CloseFrame {
                code: (code as u16).into(),
                reason: reason.to_string().into(),
            };
            let _ = socket.close(Some(frame));
            1
        }
        None => 0,
    }
}

/// Registers the TCP/WebSocket host functions on `linker`.
pub fn link_net_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "Dream",
        "tcpConnect",
        |mut caller: Caller<'_, ()>, host_ptr: i32, port: i32, timeout_ms: i32| -> Result<i32> {
            let host = read_arg_string(&mut caller, host_ptr)?;
            let response = tcp_connect(&host, port, timeout_ms);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "tcpSend",
        |mut caller: Caller<'_, ()>, handle: i32, data_ptr: i32| -> Result<i32> {
            let data = read_arg_bytes(&mut caller, data_ptr)?;
            let response = tcp_send(handle, &data);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "tcpSendText",
        |mut caller: Caller<'_, ()>, handle: i32, text_ptr: i32| -> Result<i32> {
            let text = read_arg_string(&mut caller, text_ptr)?;
            let response = tcp_send(handle, text.as_bytes());
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "tcpReceive",
        |mut caller: Caller<'_, ()>, handle: i32, max_bytes: i32| -> Result<i32> {
            let response = tcp_receive(handle, max_bytes);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap("Dream", "tcpClose", |_: Caller<'_, ()>, handle: i32| -> i32 {
        tcp_close(handle)
    })?;

    linker.func_wrap(
        "Dream",
        "wsConnect",
        |mut caller: Caller<'_, ()>, url_ptr: i32, timeout_ms: i32| -> Result<i32> {
            let url = read_arg_string(&mut caller, url_ptr)?;
            let response = ws_connect(&url, timeout_ms);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "wsSendText",
        |mut caller: Caller<'_, ()>, handle: i32, text_ptr: i32| -> Result<i32> {
            let text = read_arg_string(&mut caller, text_ptr)?;
            let response = ws_send(handle, Message::Text(text));
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "wsSendBinary",
        |mut caller: Caller<'_, ()>, handle: i32, data_ptr: i32| -> Result<i32> {
            let data = read_arg_bytes(&mut caller, data_ptr)?;
            let response = ws_send(handle, Message::Binary(data));
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "wsReceive",
        |mut caller: Caller<'_, ()>, handle: i32| -> Result<i32> {
            let response = ws_receive(handle);
            resolve_host_future_bytes(&mut caller, &response)
        },
    )?;
    linker.func_wrap(
        "Dream",
        "wsClose",
        |mut caller: Caller<'_, ()>, handle: i32, code: i32, reason_ptr: i32| -> Result<i32> {
            let reason = read_arg_string(&mut caller, reason_ptr)?;
            Ok(ws_close(handle, code, &reason))
        },
    )?;

    Ok(())
}
