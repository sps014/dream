//! Network guest C ABI.

use dream_host_abi::*;

#[no_mangle]
pub unsafe extern "C" fn httpRequest(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::http::perform_http(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_string(body).into_bytes(),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

/// Deferred variant of [`httpRequest`]: performs the request on a worker thread and completes
/// the guest future via the bound `dream_complete_foreign`. Returns 1 (work deferred) so the
/// guest loop stays parked for it; errors still complete with an encoded error payload.
#[no_mangle]
pub unsafe extern "C" fn httpRequestAsync(
    future: usize,
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> i32 {
    let args = (
        read_string(method),
        read_string(url),
        read_string(headers),
        read_string(body).into_bytes(),
    );
    std::thread::spawn(move || {
        let out =
            crate::http::perform_http(&args.0, &args.1, &args.2, args.3, timeout_ms, http_version);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn httpRequestBytes(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::http::perform_http(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_bytes(body),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

/// Deferred variant of [`httpRequestBytes`] — see [`httpRequestAsync`].
#[no_mangle]
pub unsafe extern "C" fn httpRequestBytesAsync(
    future: usize,
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> i32 {
    let args = (read_string(method), read_string(url), read_string(headers));
    let payload_in = read_bytes(body);
    std::thread::spawn(move || {
        let out = crate::http::perform_http(
            &args.0,
            &args.1,
            &args.2,
            payload_in,
            timeout_ms,
            http_version,
        );
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn httpRequestStream(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::http::open_http_stream(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_string(body).into_bytes(),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

#[no_mangle]
pub unsafe extern "C" fn httpRequestStreamBytes(
    url: usize,
    method: usize,
    headers: usize,
    body: usize,
    timeout_ms: i32,
    http_version: i32,
) -> usize {
    let out = crate::http::open_http_stream(
        &read_string(method),
        &read_string(url),
        &read_string(headers),
        read_bytes(body),
        timeout_ms,
        http_version,
    );
    alloc_bytes(&out)
}

#[no_mangle]
pub extern "C" fn httpReadChunk(handle: i32, max_bytes: i32) -> usize {
    alloc_bytes(&crate::http::http_read_chunk(handle, max_bytes))
}

#[no_mangle]
pub extern "C" fn httpCloseStream(handle: i32) -> i32 {
    crate::http::http_close_stream(handle)
}

#[no_mangle]
pub unsafe extern "C" fn tcpConnect(host: usize, port: i32, timeout_ms: i32) -> usize {
    alloc_bytes(&crate::net::tcp_connect(
        &read_string(host),
        port,
        timeout_ms,
    ))
}

#[no_mangle]
pub unsafe extern "C" fn tcpSend(handle: i32, data: usize) -> usize {
    alloc_bytes(&crate::net::tcp_send(handle, &read_bytes(data)))
}

#[no_mangle]
pub unsafe extern "C" fn tcpSendText(handle: i32, text: usize) -> usize {
    alloc_bytes(&crate::net::tcp_send(handle, read_string(text).as_bytes()))
}

#[no_mangle]
pub unsafe extern "C" fn tcpReceive(handle: i32, max_bytes: i32) -> usize {
    alloc_bytes(&crate::net::tcp_receive(handle, max_bytes))
}

#[no_mangle]
pub extern "C" fn tcpClose(handle: i32) -> i32 {
    crate::net::tcp_close(handle)
}

#[no_mangle]
pub unsafe extern "C" fn wsConnect(url: usize, timeout_ms: i32) -> usize {
    alloc_bytes(&crate::net::ws_connect(&read_string(url), timeout_ms))
}

#[no_mangle]
pub unsafe extern "C" fn wsConnectAsync(future: usize, url: usize, timeout_ms: i32) -> i32 {
    let url = read_string(url);
    std::thread::spawn(move || {
        let out = crate::net::ws_connect(&url, timeout_ms);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn wsSendText(handle: i32, text: usize) -> usize {
    alloc_bytes(&crate::net::ws_send_text(handle, &read_string(text)))
}

#[no_mangle]
pub unsafe extern "C" fn wsSendTextAsync(future: usize, handle: i32, text: usize) -> i32 {
    let text = read_string(text);
    std::thread::spawn(move || {
        let out = crate::net::ws_send_text(handle, &text);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn wsSendBinary(handle: i32, data: usize) -> usize {
    alloc_bytes(&crate::net::ws_send_binary(handle, &read_bytes(data)))
}

#[no_mangle]
pub unsafe extern "C" fn wsSendBinaryAsync(future: usize, handle: i32, data: usize) -> i32 {
    let data = read_bytes(data);
    std::thread::spawn(move || {
        let out = crate::net::ws_send_binary(handle, &data);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn wsReceive(handle: i32) -> usize {
    alloc_bytes(&crate::net::ws_receive(handle))
}

#[no_mangle]
pub extern "C" fn wsReceiveAsync(future: usize, handle: i32) -> i32 {
    std::thread::spawn(move || {
        let out = crate::net::ws_receive(handle);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn wsClose(handle: i32, code: i32, reason: usize) -> i32 {
    crate::net::ws_close(handle, code, &read_string(reason))
}

#[no_mangle]
pub unsafe extern "C" fn httpServerListen(
    host: usize,
    port: i32,
    tls_cert: usize,
    tls_key: usize,
) -> usize {
    alloc_bytes(&crate::http_server::listen(
        &read_string(host),
        port,
        &read_string(tls_cert),
        &read_string(tls_key),
    ))
}

#[no_mangle]
pub extern "C" fn httpServerAccept(server: i32) -> usize {
    alloc_bytes(&crate::http_server::accept(server))
}

#[no_mangle]
pub extern "C" fn httpServerAcceptAsync(future: usize, server: i32) -> i32 {
    std::thread::spawn(move || {
        let out = crate::http_server::accept(server);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub extern "C" fn httpServerReadBody(req: i32, max_bytes: i32) -> usize {
    alloc_bytes(&crate::http_server::read_body(req, max_bytes))
}

#[no_mangle]
pub unsafe extern "C" fn httpServerRespond(
    req: i32,
    status: i32,
    headers: usize,
    body: usize,
) -> i32 {
    crate::http_server::respond(req, status, &read_string(headers), read_bytes(body))
}

#[no_mangle]
pub unsafe extern "C" fn httpServerStartStream(req: i32, status: i32, headers: usize) -> i32 {
    crate::http_server::start_stream(req, status, &read_string(headers))
}

#[no_mangle]
pub unsafe extern "C" fn httpServerWriteChunk(req: i32, data: usize) -> i32 {
    crate::http_server::write_chunk(req, read_bytes(data))
}

#[no_mangle]
pub extern "C" fn httpServerWriteChunkAsync(future: usize, req: i32, data: usize) -> i32 {
    let bytes = unsafe { read_bytes(data) };
    std::thread::spawn(move || {
        let out = crate::http_server::write_chunk(req, bytes);
        complete_foreign_future(future, out as usize);
    });
    1
}

#[no_mangle]
pub extern "C" fn httpServerEndStream(req: i32) -> i32 {
    crate::http_server::end_stream(req)
}

#[no_mangle]
pub extern "C" fn httpServerWsUpgrade(req: i32) -> i32 {
    crate::http_server::ws_upgrade(req)
}

#[no_mangle]
pub unsafe extern "C" fn httpServerWsSend(handle: i32, kind: i32, data: usize) -> usize {
    alloc_bytes(&crate::http_server::ws_send(handle, kind, read_bytes(data)))
}

#[no_mangle]
pub unsafe extern "C" fn httpServerWsSendAsync(
    future: usize,
    handle: i32,
    kind: i32,
    data: usize,
) -> i32 {
    let bytes = read_bytes(data);
    std::thread::spawn(move || {
        let out = crate::http_server::ws_send(handle, kind, bytes);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub extern "C" fn httpServerWsReceive(handle: i32) -> usize {
    alloc_bytes(&crate::http_server::ws_receive(handle))
}

#[no_mangle]
pub extern "C" fn httpServerWsReceiveAsync(future: usize, handle: i32) -> i32 {
    std::thread::spawn(move || {
        let out = crate::http_server::ws_receive(handle);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}

#[no_mangle]
pub unsafe extern "C" fn httpServerWsClose(handle: i32, code: i32, reason: usize) -> i32 {
    crate::http_server::ws_close(handle, code, &read_string(reason))
}

#[no_mangle]
pub extern "C" fn httpServerParseMultipart(req: i32) -> i32 {
    crate::http_server::parse_multipart(req)
}

#[no_mangle]
pub unsafe extern "C" fn httpServerMultipartField(req: i32, name: usize) -> usize {
    alloc_bytes(&crate::http_server::multipart_field(
        req,
        &read_string(name),
    ))
}

#[no_mangle]
pub unsafe extern "C" fn httpServerMultipartFile(req: i32, name: usize) -> usize {
    alloc_bytes(&crate::http_server::multipart_file(req, &read_string(name)))
}

#[no_mangle]
pub extern "C" fn httpServerShutdown(server: i32) -> i32 {
    crate::http_server::shutdown(server)
}

#[no_mangle]
pub extern "C" fn httpServerWait(server: i32) -> usize {
    alloc_bytes(&crate::http_server::wait(server))
}

#[no_mangle]
pub extern "C" fn httpServerWaitAsync(future: usize, server: i32) -> i32 {
    std::thread::spawn(move || {
        let out = crate::http_server::wait(server);
        let payload = alloc_bytes(&out);
        complete_foreign_future(future, payload);
    });
    1
}
