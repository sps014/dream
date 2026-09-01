//! Stub `system.webview` host when the `webview` Cargo feature is off (no wry / WebKitGTK).

fn unavailable() {
    eprintln!(
        "system.webview is not in this Dream build (compiled without `--features webview`). \
         Install libwebkit2gtk-4.1-dev and rebuild with the default features."
    );
}

pub(crate) fn create_webview(_title: &str, _width: i32, _height: i32) -> i32 {
    unavailable();
    -1
}

pub(crate) fn load_url(_id: i32, _url: &str) -> i32 {
    -1
}

pub(crate) fn load_html(_id: i32, _html: &str) -> i32 {
    -1
}

pub(crate) fn load_file(_id: i32, _path: &str) -> i32 {
    -1
}

pub(crate) fn close(_id: i32) {}

pub(crate) fn tick(_id: i32) -> Vec<u8> {
    Vec::new()
}

pub(crate) fn close_requested(_id: i32) -> bool {
    true
}

pub(crate) fn poll_messages(_id: i32) -> Vec<u8> {
    Vec::new()
}

pub(crate) fn reply(_id: i32, _reply_id: i32, _body: &str) {}

pub(crate) fn reply_bytes(_id: i32, _reply_id: i32, _body: &[u8]) {}

pub(crate) fn reply_err(_id: i32, _reply_id: i32, _message: &str) {}

pub(crate) fn emit(_id: i32, _channel: &str, _body: &str) {}

pub(crate) fn emit_bytes(_id: i32, _channel: &str, _body: &[u8]) {}

pub(crate) fn eval_js(_id: i32, _js: &str) -> Vec<u8> {
    Vec::new()
}
