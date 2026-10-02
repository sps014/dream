//! Window events forwarded to Dream (`on_resize`, `on_focus`, ...) and to page JS
//! (`Dream.on("__dream.window", …)`).
//!
//! Each event is a wire message of kind 5: the channel names the event and the body holds its
//! values, one per line. Resizes arrive in bursts while the user drags, so only the latest
//! `resized` per view stays queued.

use std::collections::VecDeque;

use winit::event::WindowEvent;
use winit::window::{Theme, Window};

use super::{dispatch_to_page, IpcKind, IpcMessage, WebViewEntry};

/// The geometry last reported to Dream. winit sends no event for programmatic resizes/moves (and
/// none at all for minimize/maximize), so every pump diffs the live window against this.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct WindowState {
    size: (f64, f64),
    pos: (f64, f64),
    minimized: bool,
    maximized: bool,
}

impl WindowState {
    pub(super) fn new(window: &Window) -> Self {
        let mut state = WindowState::default();
        let _ = sync(window, &mut state);
        state
    }
}

pub(super) fn event(channel: &str, body: String) -> IpcMessage {
    IpcMessage {
        kind: IpcKind::Window,
        binary: false,
        reply_id: 0,
        channel: channel.to_string(),
        body: body.into_bytes(),
    }
}

fn flag(b: bool) -> String {
    if b { "1" } else { "0" }.to_string()
}

/// Geometry events for whatever changed since `state` was last updated.
pub(super) fn sync(window: &Window, state: &mut WindowState) -> Vec<IpcMessage> {
    let mut out = Vec::new();
    let scale = window.scale_factor();
    let minimized = window.is_minimized().unwrap_or(false);
    if minimized != state.minimized {
        state.minimized = minimized;
        out.push(event("minimized", flag(minimized)));
    }
    let maximized = window.is_maximized();
    if maximized != state.maximized {
        state.maximized = maximized;
        out.push(event("maximized", flag(maximized)));
    }
    // A minimized window reports a zero or stale frame; its size comes back on restore.
    if minimized {
        return out;
    }
    let size = window.inner_size().to_logical::<f64>(scale);
    let size = (size.width.round(), size.height.round());
    if size != state.size {
        state.size = size;
        out.push(event("resized", format!("{}\n{}", size.0, size.1)));
    }
    if let Ok(pos) = window.outer_position() {
        let pos = pos.to_logical::<f64>(scale);
        let pos = (pos.x.round(), pos.y.round());
        if pos != state.pos {
            state.pos = pos;
            out.push(event("moved", format!("{}\n{}", pos.0, pos.1)));
        }
    }
    out
}

/// Events for one winit `WindowEvent`.
pub(super) fn translate(
    window: &Window,
    state: &mut WindowState,
    ev: &WindowEvent,
) -> Vec<IpcMessage> {
    let mut out = Vec::new();
    match ev {
        WindowEvent::Resized(_) | WindowEvent::Moved(_) => out = sync(window, state),
        WindowEvent::Focused(focused) => out.push(event("focused", flag(*focused))),
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            out.push(event("scale", scale_factor.to_string()));
        }
        WindowEvent::ThemeChanged(theme) => {
            out.push(event("theme", flag(*theme == Theme::Dark)));
        }
        WindowEvent::CloseRequested => out.push(event("close_requested", String::new())),
        _ => {}
    }
    out
}

/// Page payload: `{ "type": …, … }` with the event's values as named fields.
pub(super) fn page_json(msg: &IpcMessage) -> String {
    let body = String::from_utf8_lossy(&msg.body);
    let mut parts = body.lines();
    let mut next_num = || {
        parts
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let value = match msg.channel.as_str() {
        "resized" => serde_json::json!({ "type": "resized", "width": next_num(), "height": next_num() }),
        "moved" => serde_json::json!({ "type": "moved", "x": next_num(), "y": next_num() }),
        "focused" => serde_json::json!({ "type": "focused", "focused": next_num() != 0.0 }),
        "minimized" => serde_json::json!({ "type": "minimized", "minimized": next_num() != 0.0 }),
        "maximized" => serde_json::json!({ "type": "maximized", "maximized": next_num() != 0.0 }),
        "scale" => serde_json::json!({ "type": "scale", "scale": next_num() }),
        "theme" => serde_json::json!({ "type": "theme", "dark": next_num() != 0.0 }),
        other => serde_json::json!({ "type": other }),
    };
    value.to_string()
}

/// Queue `msg` for Dream and tell the page, replacing an older queued `resized`.
pub(super) fn deliver(entry: &mut WebViewEntry, msg: IpcMessage) {
    dispatch_to_page(entry, "__dream.window", &page_json(&msg));
    push_coalesced(&mut entry.pending, msg);
}

fn push_coalesced(pending: &mut VecDeque<IpcMessage>, msg: IpcMessage) {
    if msg.kind == IpcKind::Window && msg.channel == "resized" {
        pending.retain(|m| !(m.kind == IpcKind::Window && m.channel == "resized"));
    }
    pending.push_back(msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_events_coalesce() {
        let mut q = VecDeque::new();
        push_coalesced(&mut q, event("resized", "1\n1".into()));
        push_coalesced(&mut q, event("focused", "1".into()));
        push_coalesced(&mut q, event("resized", "2\n2".into()));
        let channels: Vec<_> = q.iter().map(|m| m.channel.clone()).collect();
        assert_eq!(channels, ["focused", "resized"]);
        assert_eq!(q.back().unwrap().body, b"2\n2");
    }

    #[test]
    fn page_payloads() {
        assert_eq!(
            page_json(&event("resized", "800\n600".into())),
            r#"{"height":600.0,"type":"resized","width":800.0}"#
        );
        assert_eq!(
            page_json(&event("theme", "1".into())),
            r#"{"dark":true,"type":"theme"}"#
        );
        assert_eq!(page_json(&event("close_requested", String::new())), r#"{"type":"close_requested"}"#);
    }
}
