//! `Dream.dialog.*` from page JS: native dialogs parented to the page's window and answered from
//! `tick`, so the Dream run loop keeps going while they are open. Off unless Dream sets
//! `view.page_dialogs = true`, so a remote page loaded with `load_url` cannot open dialogs.

use serde_json::{json, Value as JsonValue};

use crate::desktop::dialog;
use crate::desktop::wire::{DialogKind, DialogRequest, Outcome};

pub(super) const CHANNEL: &str = "__dream.dialog";

pub(super) const DISABLED: &str =
    "page dialogs are off; set `view.page_dialogs = true` in Dream to allow them";

/// A page request whose dialog is open; `reply_id` resolves the page's promise.
pub(super) struct Parked {
    pub reply_id: i32,
    handle: i32,
    kind: DialogKind,
}

fn kind_from_method(method: &str) -> Option<DialogKind> {
    Some(match method {
        "openFile" => DialogKind::PickFile,
        "openFiles" => DialogKind::PickFiles,
        "openFolder" => DialogKind::PickFolder,
        "openFolders" => DialogKind::PickFolders,
        "saveFile" => DialogKind::SaveFile,
        "message" => DialogKind::Message,
        _ => return None,
    })
}

fn text(opts: &JsonValue, key: &str) -> String {
    opts.get(key)
        .and_then(JsonValue::as_str)
        .unwrap_or("")
        .to_string()
}

/// `{ method, opts: { title, text, directory, fileName, filters: [{ name, extensions }], level,
/// buttons } }`.
fn parse(body: &str, parent: i32) -> Result<(DialogKind, DialogRequest), String> {
    let value: JsonValue =
        serde_json::from_str(body).map_err(|e| format!("bad dialog request: {e}"))?;
    let method = value.get("method").and_then(JsonValue::as_str).unwrap_or("");
    let kind = kind_from_method(method)
        .ok_or_else(|| format!("unknown dialog method: {method}"))?;
    let opts = value.get("opts").cloned().unwrap_or(JsonValue::Null);
    let filters = opts
        .get("filters")
        .and_then(JsonValue::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|f| {
                    let name = f.get("name")?.as_str()?.to_string();
                    let exts: Vec<String> = f
                        .get("extensions")?
                        .as_array()?
                        .iter()
                        .filter_map(|e| e.as_str())
                        .map(|e| e.trim_start_matches('.').to_string())
                        .filter(|e| !e.is_empty())
                        .collect();
                    (!name.is_empty() && !exts.is_empty()).then_some((name, exts))
                })
                .collect()
        })
        .unwrap_or_default();
    let level = match text(&opts, "level").as_str() {
        "warning" => 1,
        "error" => 2,
        _ => 0,
    };
    let buttons = match text(&opts, "buttons").as_str() {
        "okCancel" => 1,
        "yesNo" => 2,
        "yesNoCancel" => 3,
        _ => 0,
    };
    Ok((
        kind,
        DialogRequest {
            title: text(&opts, "title"),
            text: text(&opts, "text"),
            directory: text(&opts, "directory"),
            file_name: text(&opts, "fileName"),
            filters,
            level,
            buttons,
            parent,
        },
    ))
}

pub(super) fn start(body: &str, parent: i32, reply_id: i32) -> Result<Parked, String> {
    let (kind, req) = parse(body, parent)?;
    Ok(Parked {
        reply_id,
        handle: dialog::start(kind, &req),
        kind,
    })
}

/// The JSON reply once the dialog closed, `None` while it is open.
pub(super) fn poll(parked: &Parked) -> Option<String> {
    dialog::poll(parked.handle).map(|outcome| reply_json(parked.kind, &outcome))
}

pub(super) fn forget(parked: &Parked) {
    dialog::forget(parked.handle);
}

fn reply_json(kind: DialogKind, outcome: &Outcome) -> String {
    let value = match (kind, outcome) {
        (DialogKind::Message, Outcome::Button(name)) => json!(name),
        (DialogKind::Message, _) => json!("cancel"),
        (DialogKind::PickFiles | DialogKind::PickFolders, Outcome::Paths(paths)) => json!(paths),
        (_, Outcome::Paths(paths)) => json!(paths.first()),
        _ => JsonValue::Null,
    };
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_page_requests() {
        let body = r#"{"method":"openFiles","opts":{"title":"Pick","filters":[{"name":"Images","extensions":["png",".jpg"]},{"name":"","extensions":["x"]}]}}"#;
        let (kind, req) = parse(body, 3).unwrap();
        assert_eq!(kind, DialogKind::PickFiles);
        assert_eq!(req.title, "Pick");
        assert_eq!(req.parent, 3);
        assert_eq!(
            req.filters,
            vec![("Images".to_string(), vec!["png".to_string(), "jpg".to_string()])]
        );
        let (kind, req) =
            parse(r#"{"method":"message","opts":{"text":"Sure?","level":"warning","buttons":"yesNo"}}"#, 1)
                .unwrap();
        assert_eq!(kind, DialogKind::Message);
        assert_eq!((req.text.as_str(), req.level, req.buttons), ("Sure?", 1, 2));
        assert!(parse(r#"{"method":"format"}"#, 1).is_err());
        assert!(parse("not json", 1).is_err());
    }

    #[test]
    fn reply_shapes() {
        let paths = Outcome::Paths(vec!["/a".into(), "/b".into()]);
        assert_eq!(reply_json(DialogKind::PickFile, &paths), r#""/a""#);
        assert_eq!(reply_json(DialogKind::PickFiles, &paths), r#"["/a","/b"]"#);
        assert_eq!(reply_json(DialogKind::SaveFile, &Outcome::Cancelled), "null");
        assert_eq!(reply_json(DialogKind::Message, &Outcome::Button("yes")), r#""yes""#);
        assert_eq!(reply_json(DialogKind::Message, &Outcome::Cancelled), r#""cancel""#);
    }
}
