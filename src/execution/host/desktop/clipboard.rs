//! System clipboard (`clipboard-rs`): text, HTML, PNG images, file lists and custom formats.

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, RustImageData};

use super::wire::ClipKind;

fn context() -> Result<ClipboardContext, String> {
    ClipboardContext::new().map_err(|e| e.to_string())
}

/// `None` when the format is missing or unreadable.
pub(crate) fn get(kind: ClipKind, format: &str) -> Option<Vec<u8>> {
    let ctx = context().ok()?;
    match kind {
        ClipKind::Text => ctx.get_text().ok().map(String::into_bytes),
        ClipKind::Html => ctx.get_html().ok().map(String::into_bytes),
        ClipKind::Image => {
            let image = ctx.get_image().ok()?;
            image.to_png().ok().map(|png| png.get_bytes().to_vec())
        }
        ClipKind::Files => ctx.get_files().ok().map(|f| f.join("\n").into_bytes()),
        ClipKind::Data => ctx.get_buffer(format).ok(),
        ClipKind::Formats => ctx
            .available_formats()
            .ok()
            .map(|f| f.join("\n").into_bytes()),
    }
}

pub(crate) fn set(kind: ClipKind, format: &str, data: &[u8]) -> Result<(), String> {
    let ctx = context()?;
    let text = || String::from_utf8_lossy(data).into_owned();
    let result = match kind {
        ClipKind::Text => ctx.set_text(text()),
        ClipKind::Html => {
            let html = text();
            ctx.set(vec![
                ClipboardContent::Text(strip_tags(&html)),
                ClipboardContent::Html(html),
            ])
        }
        ClipKind::Image => {
            let image = RustImageData::from_bytes(data).map_err(|e| e.to_string())?;
            ctx.set_image(image)
        }
        ClipKind::Files => {
            let files: Vec<String> = text()
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            if files.is_empty() {
                ctx.clear()
            } else {
                ctx.set_files(files)
            }
        }
        ClipKind::Data => ctx.set_buffer(format, data.to_vec()),
        ClipKind::Formats => return Err("clipboard formats are read-only".into()),
    };
    result.map_err(|e| e.to_string())
}

pub(crate) fn has(format: &str) -> bool {
    context()
        .ok()
        .and_then(|ctx| ctx.available_formats().ok())
        .is_some_and(|formats| formats.iter().any(|f| f == format))
}

pub(crate) fn clear() -> Result<(), String> {
    context()?.clear().map_err(|e| e.to_string())
}

/// Plain-text fallback written next to HTML so apps without HTML paste still get the text.
pub(crate) fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_entities() {
        assert_eq!(strip_tags("<b>a &amp; b</b><br/>c&lt;d"), "a & bc<d");
    }
}
