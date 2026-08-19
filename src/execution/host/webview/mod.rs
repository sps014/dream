//! Native `system.webview` host — wry WebView inside a winit window, pumped cooperatively.
//!
//! Registry and event loop are thread-local: `wry::WebView` / winit types are `!Send`/`!Sync`,
//! and `dream run` is single-threaded on the main thread.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use indexmap::IndexMap;
use serde_json::Value as JsonValue;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::pump_events::EventLoopExtPumpEvents;
use winit::window::{Window, WindowId};
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::http::Request;
use wry::{Rect, WebViewBuilder};

thread_local! {
    static EVENT_LOOP: RefCell<Option<EventLoop<()>>> = const { RefCell::new(None) };
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry {
        next_id: 1,
        entries: IndexMap::new(),
        window_to_id: IndexMap::new(),
    });
}

#[derive(Clone)]
enum IpcKind {
    Event,
    Invoke,
}

struct IpcMessage {
    kind: IpcKind,
    /// When true, `body` is opaque bytes; when false, UTF-8 text.
    binary: bool,
    reply_id: i32,
    channel: String,
    body: Vec<u8>,
}

struct WebViewEntry {
    window: Window,
    webview: wry::WebView,
    close_requested: bool,
    /// Set by wry page-load callbacks once navigation has committed (pending eval queue flushed).
    page_ready: Arc<AtomicBool>,
    pending: VecDeque<IpcMessage>,
}

struct Registry {
    next_id: u32,
    entries: IndexMap<u32, WebViewEntry>,
    window_to_id: IndexMap<WindowId, u32>,
}

fn with_event_loop<R>(f: impl FnOnce(&mut EventLoop<()>) -> R) -> Result<R, String> {
    EVENT_LOOP.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(EventLoop::new().map_err(|e| format!("WebView EventLoop: {e}"))?);
        }
        Ok(f(slot.as_mut().unwrap()))
    })
}

struct WindowCreateApp {
    title: String,
    width: u32,
    height: u32,
    window: Option<Window>,
}

impl ApplicationHandler for WindowCreateApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(self.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.width.max(1) as f64,
                self.height.max(1) as f64,
            ));
        match event_loop.create_window(attrs) {
            Ok(w) => self.window = Some(w),
            Err(e) => eprintln!("Dream webviewCreate: window create failed: {e}"),
        }
    }

    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _id: WindowId, _event: WindowEvent) {}
}

struct PumpApp;

impl ApplicationHandler for PumpApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                REGISTRY.with(|cell| {
                    let mut reg = cell.borrow_mut();
                    if let Some(id) = reg.window_to_id.get(&window_id).copied() {
                        if let Some(entry) = reg.entries.get_mut(&id) {
                            entry.close_requested = true;
                        }
                    }
                });
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                REGISTRY.with(|cell| {
                    let mut reg = cell.borrow_mut();
                    if let Some(id) = reg.window_to_id.get(&window_id).copied() {
                        if let Some(entry) = reg.entries.get_mut(&id) {
                            let _ = entry.webview.set_bounds(full_window_bounds(&entry.window));
                        }
                    }
                });
            }
            _ => {}
        }
    }
}

fn pump() {
    pump_for(Duration::ZERO);
}

fn pump_for(timeout: Duration) {
    let _ = with_event_loop(|el| {
        let mut app = PumpApp;
        let _ = el.pump_app_events(Some(timeout), &mut app);
    });
    drain_ipc_inbox();
}

/// Injected page bridge: `window.Dream.emit` / `invoke` / `on` (+ raw byte variants).
const DREAM_BRIDGE: &str = r#"
(function () {
  if (window.Dream && window.Dream.__dream_webview) return;
  var pending = {};
  var nextId = 1;
  var listeners = {};
  var byteListeners = {};
  function b64encode(u8) {
    var s = '';
    var CHUNK = 0x8000;
    for (var i = 0; i < u8.length; i += CHUNK) {
      s += String.fromCharCode.apply(null, u8.subarray(i, Math.min(i + CHUNK, u8.length)));
    }
    return btoa(s);
  }
  function b64decode(s) {
    var bin = atob(s);
    var out = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }
  function toU8(body) {
    if (body instanceof Uint8Array) return body;
    if (body instanceof ArrayBuffer) return new Uint8Array(body);
    if (ArrayBuffer.isView && ArrayBuffer.isView(body)) {
      return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
    }
    return new Uint8Array(0);
  }
  function post(obj) {
    if (window.ipc && window.ipc.postMessage) {
      window.ipc.postMessage(JSON.stringify(obj));
    }
  }
  window.Dream = {
    __dream_webview: true,
    emit: function (channel, body) {
      post({ k: "e", c: String(channel), t: "s", b: body == null ? "" : String(body) });
    },
    emitBytes: function (channel, body) {
      post({ k: "e", c: String(channel), t: "b", b: b64encode(toU8(body)) });
    },
    invoke: function (channel, body) {
      var id = nextId++;
      return new Promise(function (resolve, reject) {
        pending[id] = { resolve: resolve, reject: reject, bin: false };
        post({ k: "i", id: id, c: String(channel), t: "s", b: body == null ? "" : String(body) });
      });
    },
    invokeBytes: function (channel, body) {
      var id = nextId++;
      return new Promise(function (resolve, reject) {
        pending[id] = { resolve: resolve, reject: reject, bin: true };
        post({ k: "i", id: id, c: String(channel), t: "b", b: b64encode(toU8(body)) });
      });
    },
    on: function (channel, handler) {
      var c = String(channel);
      if (!listeners[c]) listeners[c] = [];
      listeners[c].push(handler);
    },
    onBytes: function (channel, handler) {
      var c = String(channel);
      if (!byteListeners[c]) byteListeners[c] = [];
      byteListeners[c].push(handler);
    },
    __dispatch: function (channel, body) {
      var list = listeners[String(channel)] || [];
      for (var i = 0; i < list.length; i++) {
        try { list[i](body); } catch (e) { console.error(e); }
      }
    },
    __dispatchBytes: function (channel, b64) {
      var bytes = b64decode(String(b64));
      var list = byteListeners[String(channel)] || [];
      for (var i = 0; i < list.length; i++) {
        try { list[i](bytes); } catch (e) { console.error(e); }
      }
    },
    __resolve: function (id, body) {
      var p = pending[id];
      if (p) { delete pending[id]; p.resolve(body); }
    },
    __resolveBytes: function (id, b64) {
      var p = pending[id];
      if (p) { delete pending[id]; p.resolve(b64decode(String(b64))); }
    },
    __reject: function (id, message) {
      var p = pending[id];
      if (p) { delete pending[id]; p.reject(new Error(message || "invoke failed")); }
    }
  };
})();
"#;

/// Drive AppKit until wry reports page load. Until commit, `evaluate_script` only queues JS
/// (callbacks dropped) so emit/reply look dead. After ready, re-inject the bridge.
fn settle_navigation(id: i32) {
    for _ in 0..80 {
        let ready = with_entry_mut(id, |e| e.page_ready.load(Ordering::SeqCst)).unwrap_or(true);
        if ready {
            let _ = with_entry_mut(id, |e| e.webview.evaluate_script(DREAM_BRIDGE));
            pump_for(Duration::from_millis(8));
            return;
        }
        pump_for(Duration::from_millis(4));
    }
    let _ = with_entry_mut(id, |e| e.webview.evaluate_script(DREAM_BRIDGE));
    pump_for(Duration::from_millis(8));
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

fn ipc_inbox() -> &'static Mutex<VecDeque<(u32, IpcMessage)>> {
    static INBOX: OnceLock<Mutex<VecDeque<(u32, IpcMessage)>>> = OnceLock::new();
    INBOX.get_or_init(|| Mutex::new(VecDeque::new()))
}

fn b64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push(TABLE[(n & 63) as usize] as char);
        i += 3;
    }
    if data.len() - i == 1 {
        let n = (data[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if data.len() - i == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

fn b64_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let (a, b, c, d) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        let av = val(a)?;
        let bv = val(b)?;
        let cv = if c == b'=' { 0 } else { val(c)? };
        let dv = if d == b'=' { 0 } else { val(d)? };
        let n = ((av as u32) << 18) | ((bv as u32) << 12) | ((cv as u32) << 6) | (dv as u32);
        out.push(((n >> 16) & 255) as u8);
        if c != b'=' {
            out.push(((n >> 8) & 255) as u8);
        }
        if d != b'=' {
            out.push((n & 255) as u8);
        }
    }
    Some(out)
}

fn enqueue_ipc(id: u32, raw: &str) {
    let parsed: JsonValue = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let kind_s = parsed.get("k").and_then(|v| v.as_str()).unwrap_or("");
    let channel = parsed
        .get("c")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ty = parsed.get("t").and_then(|v| v.as_str()).unwrap_or("s");
    let body_field = parsed.get("b").and_then(|v| v.as_str()).unwrap_or("");
    let binary = ty == "b";
    let body = if binary {
        b64_decode(body_field).unwrap_or_default()
    } else {
        body_field.as_bytes().to_vec()
    };
    let (kind, reply_id) = match kind_s {
        "e" => (IpcKind::Event, 0),
        "i" => (
            IpcKind::Invoke,
            parsed.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        ),
        _ => return,
    };
    // IPC may arrive off the Dream thread; stage in a Sync inbox and drain on pump/poll.
    ipc_inbox()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push_back((
            id,
            IpcMessage {
                kind,
                binary,
                reply_id,
                channel,
                body,
            },
        ));
}

fn drain_ipc_inbox() {
    let staged: Vec<(u32, IpcMessage)> = ipc_inbox()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .drain(..)
        .collect();
    if staged.is_empty() {
        return;
    }
    REGISTRY.with(|cell| {
        let mut reg = cell.borrow_mut();
        for (id, msg) in staged {
            if let Some(entry) = reg.entries.get_mut(&id) {
                entry.pending.push_back(msg);
            }
        }
    });
}

fn encode_poll(messages: &[IpcMessage]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("{}\n", messages.len()).as_bytes());
    for msg in messages {
        // 0/1 = UTF-8 string event/invoke; 2/3 = raw byte event/invoke.
        let kind = match (&msg.kind, msg.binary) {
            (IpcKind::Event, false) => 0,
            (IpcKind::Invoke, false) => 1,
            (IpcKind::Event, true) => 2,
            (IpcKind::Invoke, true) => 3,
        };
        let body = &msg.body;
        out.extend_from_slice(format!("{kind}\n").as_bytes());
        out.extend_from_slice(format!("{}\n", msg.reply_id).as_bytes());
        out.extend_from_slice(msg.channel.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{}\n", body.len()).as_bytes());
        out.extend_from_slice(body);
    }
    out
}

fn encode_eval(ok: bool, payload: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let code = if ok { 0 } else { 1 };
    out.extend_from_slice(format!("{code}\n").as_bytes());
    out.extend_from_slice(payload.as_bytes());
    out
}

fn full_window_bounds(window: &Window) -> Rect {
    let scale = window.scale_factor();
    let logical: winit::dpi::LogicalSize<f64> = window.inner_size().to_logical(scale);
    // Before the first layout pass, macOS can report 0×0 — keep a usable child size.
    let width = if logical.width > 1.0 {
        logical.width
    } else {
        800.0
    };
    let height = if logical.height > 1.0 {
        logical.height
    } else {
        600.0
    };
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(width, height).into(),
    }
}

fn child_bounds(width: u32, height: u32) -> Rect {
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(width.max(1) as f64, height.max(1) as f64).into(),
    }
}

pub(crate) fn create_webview(title: &str, width: i32, height: i32) -> i32 {
    let w = width.max(1) as u32;
    let h = height.max(1) as u32;
    let created = with_event_loop(|el| {
        let mut app = WindowCreateApp {
            title: title.to_string(),
            width: w,
            height: h,
            window: None,
        };
        // macOS may need a couple of pumps before `resumed` delivers a window.
        // Prefer a short wait over many empty spins once the app is launched.
        for _ in 0..8 {
            let timeout = if app.window.is_none() {
                Duration::from_millis(1)
            } else {
                Duration::ZERO
            };
            let _ = el.pump_app_events(Some(timeout), &mut app);
            if app.window.is_some() {
                break;
            }
        }
        app.window
    });

    let window = match created {
        Ok(Some(window)) => window,
        Ok(None) => {
            eprintln!("Dream webviewCreate: no window (unavailable)");
            return -1;
        }
        Err(e) => {
            eprintln!("Dream webviewCreate: {e}");
            return -1;
        }
    };

    let id = REGISTRY.with(|cell| {
        let mut reg = cell.borrow_mut();
        let id = reg.next_id;
        reg.next_id = id.saturating_add(1);
        id
    });

    let webview = {
        let id_for_ipc = id;
        let page_ready = Arc::new(AtomicBool::new(false));
        let page_ready_cb = page_ready.clone();
        // Prefer the requested logical size: `inner_size` can still be 0×0 on macOS here.
        let bounds = child_bounds(w, h);
        // `build_as_child` keeps winit's NSView intact. Plain `build` replaces the content
        // view on macOS and panics on focus changes (winit ivars missing on WryWebViewParent).
        // Skip a placeholder `with_html` — the first `load_*` is the only navigation.
        match WebViewBuilder::new()
            .with_initialization_script(DREAM_BRIDGE)
            .with_ipc_handler(move |req: Request<String>| {
                enqueue_ipc(id_for_ipc, req.body());
            })
            .with_on_page_load_handler(move |event, _url| {
                // Wait until Finished so document scripts (and boot()) can run.
                if matches!(event, wry::PageLoadEvent::Finished) {
                    page_ready_cb.store(true, Ordering::SeqCst);
                }
            })
            .with_bounds(bounds)
            .build_as_child(&window)
        {
            Ok(wv) => {
                let _ = wv.set_bounds(bounds);
                let _ = wv.set_visible(true);
                (wv, page_ready)
            }
            Err(e) => {
                eprintln!("Dream webviewCreate: wry build failed: {e}");
                return -1;
            }
        }
    };

    let (webview, page_ready) = webview;

    let window_id = window.id();
    REGISTRY.with(|cell| {
        let mut reg = cell.borrow_mut();
        reg.window_to_id.insert(window_id, id);
        reg.entries.insert(
            id,
            WebViewEntry {
                window,
                webview,
                close_requested: false,
                page_ready,
                pending: VecDeque::new(),
            },
        );
    });
    // Sync child bounds once the window has a real layout size.
    pump();
    let _ = with_entry_mut(id as i32, |e| {
        let _ = e.webview.set_bounds(full_window_bounds(&e.window));
    });
    id as i32
}

fn with_entry_mut<R>(id: i32, f: impl FnOnce(&mut WebViewEntry) -> R) -> Option<R> {
    if id <= 0 {
        return None;
    }
    REGISTRY.with(|cell| {
        let mut reg = cell.borrow_mut();
        reg.entries.get_mut(&(id as u32)).map(f)
    })
}

pub(crate) fn load_url(id: i32, url: &str) -> i32 {
    pump();
    match with_entry_mut(id, |e| {
        e.page_ready.store(false, Ordering::SeqCst);
        let r = e.webview.load_url(url);
        let _ = e.webview.set_bounds(full_window_bounds(&e.window));
        let _ = e.webview.focus();
        e.window.set_visible(true);
        e.window.focus_window();
        r
    }) {
        Some(Ok(())) => {
            settle_navigation(id);
            0
        }
        Some(Err(err)) => {
            eprintln!("Dream webviewLoadUrl: {err}");
            1
        }
        None => 1,
    }
}

pub(crate) fn load_html(id: i32, html: &str) -> i32 {
    pump();
    match with_entry_mut(id, |e| {
        e.page_ready.store(false, Ordering::SeqCst);
        let r = e.webview.load_html(html);
        let _ = e.webview.set_bounds(full_window_bounds(&e.window));
        let _ = e.webview.focus();
        e.window.set_visible(true);
        e.window.focus_window();
        r
    }) {
        Some(Ok(())) => {
            settle_navigation(id);
            0
        }
        Some(Err(err)) => {
            eprintln!("Dream webviewLoadHtml: {err}");
            1
        }
        None => 1,
    }
}

pub(crate) fn load_file(id: i32, path: &str) -> i32 {
    pump();
    let abs: PathBuf = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        match env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(e) => {
                eprintln!("Dream webviewLoadFile: {e}");
                return 1;
            }
        }
    };
    let abs = match abs.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Dream webviewLoadFile: {}: {e}", abs.display());
            return 1;
        }
    };
    if !abs.is_file() {
        eprintln!("Dream webviewLoadFile: not a file: {}", abs.display());
        return 1;
    }
    // Load as HTML string (not file://) so WKWebView IPC / init scripts behave like wry demos.
    match std::fs::read_to_string(&abs) {
        Ok(html) => load_html(id, &html),
        Err(e) => {
            eprintln!("Dream webviewLoadFile: read {}: {e}", abs.display());
            1
        }
    }
}

pub(crate) fn close(id: i32) {
    pump();
    if id <= 0 {
        return;
    }
    REGISTRY.with(|cell| {
        let mut reg = cell.borrow_mut();
        if let Some(entry) = reg.entries.swap_remove(&(id as u32)) {
            reg.window_to_id.swap_remove(&entry.window.id());
            drop(entry);
        }
    });
}

/// One `WebView.run` iteration: pump AppKit, then return `closed\n` + poll payload.
pub(crate) fn tick(id: i32) -> Vec<u8> {
    pump_for(Duration::from_millis(16));
    let closed = with_entry_mut(id, |e| e.close_requested).unwrap_or(true);
    let messages =
        with_entry_mut(id, |e| e.pending.drain(..).collect::<Vec<_>>()).unwrap_or_default();
    let mut out = Vec::new();
    out.extend_from_slice(if closed { b"1\n" } else { b"0\n" });
    out.extend_from_slice(&encode_poll(&messages));
    out
}

pub(crate) fn close_requested(id: i32) -> bool {
    pump_for(Duration::from_millis(16));
    with_entry_mut(id, |e| e.close_requested).unwrap_or(true)
}

pub(crate) fn poll_messages(id: i32) -> Vec<u8> {
    pump();
    let messages =
        with_entry_mut(id, |e| e.pending.drain(..).collect::<Vec<_>>()).unwrap_or_default();
    encode_poll(&messages)
}

pub(crate) fn reply(id: i32, reply_id: i32, body: &str) {
    pump();
    let script = format!(
        "window.Dream && Dream.__resolve({}, {});",
        reply_id,
        js_string(body)
    );
    let _ = with_entry_mut(id, |e| e.webview.evaluate_script(&script));
}

pub(crate) fn reply_bytes(id: i32, reply_id: i32, body: &[u8]) {
    pump();
    let script = format!(
        "window.Dream && Dream.__resolveBytes({}, {});",
        reply_id,
        js_string(&b64_encode(body))
    );
    let _ = with_entry_mut(id, |e| e.webview.evaluate_script(&script));
}

pub(crate) fn reply_err(id: i32, reply_id: i32, message: &str) {
    pump();
    let script = format!(
        "window.Dream && Dream.__reject({}, {});",
        reply_id,
        js_string(message)
    );
    let _ = with_entry_mut(id, |e| e.webview.evaluate_script(&script));
}

pub(crate) fn emit(id: i32, channel: &str, body: &str) {
    pump();
    let script = format!(
        "window.Dream && Dream.__dispatch({}, {});",
        js_string(channel),
        js_string(body)
    );
    let _ = with_entry_mut(id, |e| e.webview.evaluate_script(&script));
}

pub(crate) fn emit_bytes(id: i32, channel: &str, body: &[u8]) {
    pump();
    let script = format!(
        "window.Dream && Dream.__dispatchBytes({}, {});",
        js_string(channel),
        js_string(&b64_encode(body))
    );
    let _ = with_entry_mut(id, |e| e.webview.evaluate_script(&script));
}

fn decode_eval_callback(result_json: &str) -> Result<String, String> {
    match serde_json::from_str::<JsonValue>(result_json) {
        Ok(JsonValue::String(s)) => {
            if let Some(rest) = s.strip_prefix("__dream_eval_err__:") {
                Err(rest.to_string())
            } else {
                Ok(s)
            }
        }
        Ok(JsonValue::Null) => Ok(String::new()),
        Ok(other) => Ok(other.to_string()),
        Err(_) => Ok(result_json.to_string()),
    }
}

pub(crate) fn eval_js(id: i32, js: &str) -> Vec<u8> {
    pump();
    let wrapped = format!(
        "(function(){{ try {{ var __r = (function(){{ {js} }})(); return (__r === undefined || __r === null) ? '' : String(__r); }} catch(e) {{ return '__dream_eval_err__:' + (e && e.message ? e.message : String(e)); }} }})()"
    );
    let (tx, rx) = mpsc::channel();
    let started = with_entry_mut(id, |e| {
        e.webview
            .evaluate_script_with_callback(&wrapped, move |result| {
                let _ = tx.send(decode_eval_callback(&result));
            })
    });
    match started {
        Some(Ok(())) => {}
        Some(Err(e)) => return encode_eval(false, &e.to_string()),
        None => return encode_eval(false, "invalid webview handle"),
    }

    for _ in 0..500 {
        pump_for(Duration::from_millis(2));
        match rx.try_recv() {
            Ok(Ok(s)) => return encode_eval(true, &s),
            Ok(Err(e)) => return encode_eval(false, &e),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                return encode_eval(false, "eval callback dropped");
            }
        }
    }
    encode_eval(false, "eval timed out")
}
