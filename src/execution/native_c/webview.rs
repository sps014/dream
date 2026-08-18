//! C ABI for `system.webview` host functions (`--native-c` / `-ldream`).

#![allow(clippy::missing_safety_doc)]

use super::abi::{alloc_bytes, read_bytes, read_string};
use crate::execution::host::webview;

#[no_mangle]
pub unsafe extern "C" fn webviewCreate(title: usize, width: i32, height: i32) -> i32 {
    webview::create_webview(&read_string(title), width, height)
}

#[no_mangle]
pub unsafe extern "C" fn webviewLoadUrl(id: i32, url: usize) -> i32 {
    webview::load_url(id, &read_string(url))
}

#[no_mangle]
pub unsafe extern "C" fn webviewLoadHtml(id: i32, html: usize) -> i32 {
    webview::load_html(id, &read_string(html))
}

#[no_mangle]
pub unsafe extern "C" fn webviewLoadFile(id: i32, path: usize) -> i32 {
    webview::load_file(id, &read_string(path))
}

#[no_mangle]
pub extern "C" fn webviewClose(id: i32) {
    webview::close(id);
}

#[no_mangle]
pub extern "C" fn webviewCloseRequested(id: i32) -> i32 {
    i32::from(webview::close_requested(id))
}

#[no_mangle]
pub extern "C" fn webviewTick(id: i32) -> usize {
    alloc_bytes(&webview::tick(id))
}

#[no_mangle]
pub extern "C" fn webviewPoll(id: i32) -> usize {
    alloc_bytes(&webview::poll_messages(id))
}

#[no_mangle]
pub unsafe extern "C" fn webviewReply(id: i32, reply_id: i32, body: usize) {
    webview::reply(id, reply_id, &read_string(body));
}

#[no_mangle]
pub unsafe extern "C" fn webviewReplyErr(id: i32, reply_id: i32, message: usize) {
    webview::reply_err(id, reply_id, &read_string(message));
}

#[no_mangle]
pub unsafe extern "C" fn webviewReplyBytes(id: i32, reply_id: i32, body: usize) {
    webview::reply_bytes(id, reply_id, &read_bytes(body));
}

#[no_mangle]
pub unsafe extern "C" fn webviewEmit(id: i32, channel: usize, body: usize) {
    webview::emit(id, &read_string(channel), &read_string(body));
}

#[no_mangle]
pub unsafe extern "C" fn webviewEmitBytes(id: i32, channel: usize, body: usize) {
    webview::emit_bytes(id, &read_string(channel), &read_bytes(body));
}

#[no_mangle]
pub unsafe extern "C" fn webviewEval(id: i32, js: usize) -> usize {
    alloc_bytes(&webview::eval_js(id, &read_string(js)))
}
