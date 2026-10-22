use super::{close, listen::Action, start};
use crate::{
    socket_operations as operations,
    websocket::{self, Action as Ws},
};
use dream_host_abi::wire::{Reader, VERSION, Writer};
use serde_json::Value;
use std::time::{Duration, Instant};

fn wait(id: i32) -> Vec<u8> {
    assert!(id > 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let wire = operations::OPERATIONS.result(id);
        if wire[0] != 0 {
            return wire;
        }
        assert!(Instant::now() < deadline, "HTTP server operation timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn take(id: i32) -> Vec<u8> {
    let wire = wait(id);
    assert_eq!(wire[0], 1, "{}", String::from_utf8_lossy(&wire));
    operations::OPERATIONS.claim(id);
    operations::OPERATIONS.release(id);
    wire[1..].to_vec()
}
fn metadata(bytes: Vec<u8>) -> Value {
    serde_json::from_slice(&bytes).unwrap()
}
fn header(action: u8, id: i32) -> Writer {
    let mut writer = Writer::new();
    writer.u8(VERSION).u8(action).i32(id).i32(5000);
    writer
}
fn op(action: Action, id: i32, bytes: Vec<u8>) -> i32 {
    start(header(action as u8, id).finish(), bytes)
}
fn ws(action: Ws, id: i32, bytes: Vec<u8>) -> i32 {
    websocket::start(header(action as u8, id).finish(), bytes)
}
fn connect(port: u16) -> i32 {
    let mut request = header(Ws::Connect as u8, 0);
    request.text(&format!("ws://127.0.0.1:{port}/ws"));
    websocket::start(request.finish(), vec![])
}
fn close_socket(id: i32, code: i32, reason: &str) -> i32 {
    let mut request = header(Ws::Close as u8, id);
    request.i32(code).text(reason);
    websocket::start(request.finish(), vec![])
}
/// A receive completion as `(kind, text or bytes, close code)`.
fn event(bytes: Vec<u8>) -> (u8, Vec<u8>, i32) {
    let mut reader = Reader::new(&bytes);
    let kind = reader.u8().unwrap();
    let event = match kind {
        1 => (kind, reader.text().unwrap().as_bytes().to_vec(), 0),
        2 => (kind, reader.bytes().unwrap().to_vec(), 0),
        _ => {
            let code = reader.i32().unwrap();
            (kind, reader.text().unwrap().as_bytes().to_vec(), code)
        }
    };
    reader.finish().unwrap();
    event
}
fn handle(bytes: Vec<u8>) -> i32 {
    let mut reader = Reader::new(&bytes);
    let id = reader.i32().unwrap();
    reader.finish().unwrap();
    id
}
struct Settings<'a> {
    cert: &'a str,
    key: &'a str,
    limit_concurrency: i32,
    keep_alive_ms: i32,
    graceful_ms: i32,
    max_header_bytes: i32,
    ws_max_size: i32,
    ping_interval_ms: i32,
    ping_timeout_ms: i32,
    proxy_headers: bool,
    forwarded_allow_ips: &'a str,
}
impl Default for Settings<'_> {
    fn default() -> Self {
        Self {
            cert: "",
            key: "",
            limit_concurrency: 0,
            keep_alive_ms: 5000,
            graceful_ms: 0,
            max_header_bytes: 16384,
            ws_max_size: 16777216,
            ping_interval_ms: 20000,
            ping_timeout_ms: 20000,
            proxy_headers: true,
            forwarded_allow_ips: "127.0.0.1",
        }
    }
}
fn listen_operation(settings: Settings<'_>) -> i32 {
    let mut request = header(Action::Listen as u8, 0);
    request
        .text("127.0.0.1")
        .i32(0)
        .text(settings.cert)
        .text(settings.key)
        .i32(settings.limit_concurrency)
        .i32(2048)
        .i32(settings.keep_alive_ms)
        .i32(settings.graceful_ms)
        .i32(settings.max_header_bytes)
        .i32(settings.ws_max_size)
        .i32(32)
        .i32(settings.ping_interval_ms)
        .i32(settings.ping_timeout_ms)
        .i32(settings.proxy_headers.into())
        .i32(100)
        .text(settings.forwarded_allow_ips);
    start(request.finish(), vec![])
}
fn listen_with(settings: Settings<'_>) -> (i32, u16) {
    let bytes = take(listen_operation(settings));
    let mut reader = Reader::new(&bytes);
    let id = reader.i32().unwrap();
    let port = u16::try_from(reader.i32().unwrap()).unwrap();
    reader.finish().unwrap();
    (id, port)
}
fn listen(cert: &str, key: &str) -> (i32, u16) {
    listen_with(Settings {
        cert,
        key,
        ..Settings::default()
    })
}
fn client<T: Send + 'static>(
    work: impl std::future::Future<Output = T> + Send + 'static,
) -> std::thread::JoinHandle<T> {
    std::thread::spawn(move || operations::runtime().unwrap().block_on(work))
}
fn failure(operation: i32) -> String {
    let wire = wait(operation);
    assert_eq!(wire[0], 2, "{}", String::from_utf8_lossy(&wire));
    operations::OPERATIONS.release(operation);
    Reader::new(&wire[1..]).text().unwrap().to_owned()
}
fn read_chunk(request: i32, max: i32) -> Vec<u8> {
    let mut operation = header(Action::BodyRead as u8, request);
    operation.i32(max);
    take(start(operation.finish(), vec![]))
}
fn form(request: i32, max_files: i32, max_fields: i32, max_part_size: i32) -> i32 {
    let mut operation = header(Action::Form as u8, request);
    operation.i32(max_files).i32(max_fields).i32(max_part_size);
    start(operation.finish(), vec![])
}
fn default_form(request: i32) -> i32 {
    form(request, 1000, 1000, 1048576)
}
/// Starts the request record's id, after its version byte.
fn accept(server: i32) -> i32 {
    let record = take(op(Action::Accept, server, vec![]));
    assert_eq!(record[0], VERSION);
    Reader::new(&record[1..]).i32().unwrap()
}
fn respond(action: Action, id: i32, bytes: &[u8]) {
    let mut operation = header(action as u8, id);
    operation
        .i32(200)
        .i32(2)
        .text("x-repeat")
        .text("a")
        .text("x-repeat")
        .text("b");
    take(start(operation.finish(), bytes.to_vec()));
}
#[test]
fn round_trip_multipart_and_backpressured_streaming() {
    let (server, port) = listen("", "");
    let client = std::thread::spawn(move || {
        operations::runtime().unwrap().block_on(async move {
        let body = "--dream\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nhello\r\n--dream\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nfile data\r\n--dream--\r\n";
        let response = reqwest::Client::new().post(format!("http://127.0.0.1:{port}/upload?x=1")).header("content-type", "multipart/form-data; boundary=dream").body(body).send().await.unwrap();
        assert_eq!(response.headers().get_all("x-repeat").iter().count(), 2);
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"onetwo");
    })
    });
    let request = accept(server);
    let parts = metadata(take(default_form(request)));
    assert_eq!(parts["fields"][0]["name"], "title");
    assert_eq!(parts["fields"][0]["value"], "hello");
    let file = &parts["files"][0];
    assert_eq!(file["filename"], "a.txt");
    assert_eq!(file["size"], 9);
    let upload = file["handle"].as_i64().unwrap() as i32;
    let read_operation = |max: i32| {
        let mut operation = header(Action::UploadRead as u8, upload);
        operation.i32(max);
        start(operation.finish(), vec![])
    };
    let read = |max: i32| take(read_operation(max));
    assert_eq!(read(4), b"file");
    assert_eq!(read(-1), b" data");
    let mut seek = header(Action::UploadSeek as u8, upload);
    seek.i64(5);
    take(start(seek.finish(), vec![]));
    assert_eq!(read(-1), b"data");
    respond(Action::StreamBegin, request, b"");
    take(op(Action::StreamWrite, request, b"one".to_vec()));
    take(op(Action::StreamWrite, request, b"two".to_vec()));
    take(op(Action::StreamEnd, request, vec![]));
    client.join().unwrap();
    super::release(request);
    assert_eq!(failure(read_operation(1)), "ECLOSED");
    close(server);
}
#[test]
fn bodies_are_uncapped_cached_and_streamed_once() {
    let (server, port) = listen("", "");
    let large = vec![7u8; 20 << 20];
    let sent = large.clone();
    let poster = client(async move {
        let url = format!("http://127.0.0.1:{port}/");
        let http = reqwest::Client::new();
        let first = http.post(&url).body(sent).send().await.unwrap().status();
        let second = http
            .post(&url)
            .body("streamed")
            .send()
            .await
            .unwrap()
            .status();
        (first, second)
    });
    let request = accept(server);
    assert_eq!(
        take(op(Action::ReadBody, request, vec![])).len(),
        large.len()
    );
    assert_eq!(read_chunk(request, 1 << 30).len(), large.len());
    assert!(read_chunk(request, 16).is_empty());
    assert_eq!(
        take(op(Action::ReadBody, request, vec![])).len(),
        large.len()
    );
    respond(Action::Respond, request, b"");
    super::release(request);
    let request = accept(server);
    let mut streamed = Vec::new();
    loop {
        let chunk = read_chunk(request, 3);
        if chunk.is_empty() {
            break;
        }
        assert!(chunk.len() <= 3);
        streamed.extend(chunk);
    }
    assert_eq!(streamed, b"streamed");
    assert_eq!(failure(op(Action::ReadBody, request, vec![])), "ECLOSED");
    respond(Action::Respond, request, b"");
    super::release(request);
    let (first, second) = poster.join().unwrap();
    assert_eq!((first.as_u16(), second.as_u16()), (200, 200));
    close(server);
}
#[test]
fn form_limits_reject_overflow() {
    let (server, port) = listen("", "");
    let field = |name: &str, value: &str| {
        format!("--b\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
    };
    let file = |name: &str| {
        format!(
            "--b\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}.bin\"\r\n\r\nx\r\n"
        )
    };
    let bodies = vec![
        format!("{}{}--b--\r\n", field("a", "1"), field("b", "2")),
        format!("{}{}--b--\r\n", file("a"), file("b")),
        format!("{}--b--\r\n", field("a", &"x".repeat(32))),
    ];
    let poster = client(async move {
        let http = reqwest::Client::new();
        for body in bodies {
            http.post(format!("http://127.0.0.1:{port}/"))
                .header("content-type", "multipart/form-data; boundary=b")
                .body(body)
                .send()
                .await
                .unwrap();
        }
    });
    for (max_files, max_fields, max_part_size) in
        [(1000, 1, 1048576), (1, 1000, 1048576), (1000, 1000, 16)]
    {
        let request = accept(server);
        assert_eq!(
            failure(form(request, max_files, max_fields, max_part_size)),
            "EINVAL"
        );
        respond(Action::Respond, request, b"");
        super::release(request);
    }
    poster.join().unwrap();
    close(server);
}
#[test]
fn large_file_parts_spool_to_disk_until_release() {
    let (server, port) = listen("", "");
    let poster = client(async move {
        let mut body =
            b"--b\r\nContent-Disposition: form-data; name=\"f\"; filename=\"big.bin\"\r\n\r\n"
                .to_vec();
        body.extend(std::iter::repeat_n(9u8, 3 << 20));
        body.extend_from_slice(b"\r\n--b--\r\n");
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/"))
            .header("content-type", "multipart/form-data; boundary=b")
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    });
    let request = accept(server);
    let parts = metadata(take(default_form(request)));
    assert_eq!(parts["files"][0]["size"], 3 << 20);
    let handle = parts["files"][0]["handle"].as_i64().unwrap() as i32;
    assert!(super::state::lock(&super::state::upload(handle).unwrap()).is_rolled());
    respond(Action::Respond, request, b"");
    super::release(request);
    assert!(super::state::upload(handle).is_err());
    assert_eq!(poster.join().unwrap(), 200);
    close(server);
}
#[test]
fn websocket_ping_timeout_closes_silent_peer() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (server, port) = listen_with(Settings {
        ping_interval_ms: 50,
        ping_timeout_ms: 100,
        ..Settings::default()
    });
    let peer = client(async move {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").await.unwrap();
        let mut sink = vec![0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while stream.read(&mut sink).await.is_ok_and(|n| n > 0) {}
        })
        .await;
    });
    let request = accept(server);
    let socket = handle(take(op(Action::Upgrade, request, vec![])));
    let (kind, _, code) = event(take(ws(Ws::Receive, socket, vec![])));
    assert_eq!((kind, code), (3, 1011));
    websocket::close(socket);
    super::release(request);
    peer.join().unwrap();
    close(server);
}
/// Accepts one request and returns its id with the client host, port and scheme fields.
fn accept_client(server: i32) -> (i32, String, i32, String) {
    let record = take(op(Action::Accept, server, vec![]));
    let mut reader = Reader::new(&record);
    assert_eq!(reader.u8().unwrap(), VERSION);
    let id = reader.i32().unwrap();
    if reader.u8().unwrap() == 0 {
        reader.text().unwrap();
    }
    reader.text().unwrap();
    reader.text().unwrap();
    for _ in 0..reader.i32().unwrap() {
        reader.text().unwrap();
        reader.text().unwrap();
    }
    let host = reader.text().unwrap().to_owned();
    let port = reader.i32().unwrap();
    let scheme = reader.text().unwrap().to_owned();
    reader.finish().unwrap();
    (id, host, port, scheme)
}
#[test]
fn proxy_headers_apply_only_to_trusted_peers() {
    for (allowed, expected_host, expected_scheme) in [
        ("127.0.0.1", "203.0.113.7", "https"),
        ("10.0.0.0/8", "127.0.0.1", "http"),
    ] {
        let (server, port) = listen_with(Settings {
            forwarded_allow_ips: allowed,
            ..Settings::default()
        });
        let client = client(async move {
            reqwest::Client::new()
                .get(format!("http://127.0.0.1:{port}/"))
                .header("x-forwarded-for", "203.0.113.7, 127.0.0.1")
                .header("x-forwarded-proto", "https")
                .send()
                .await
                .unwrap()
                .status()
        });
        let (request, host, client_port, scheme) = accept_client(server);
        assert_eq!((host.as_str(), scheme.as_str()), (expected_host, expected_scheme));
        assert_eq!(client_port == 0, expected_host != "127.0.0.1");
        respond(Action::Respond, request, b"ok");
        super::release(request);
        assert_eq!(client.join().unwrap(), 200);
        close(server);
    }
}
#[test]
fn serves_http2_with_prior_knowledge() {
    let (server, port) = listen("", "");
    let client = client(async move {
        let http = reqwest::Client::builder()
            .http2_prior_knowledge()
            .build()
            .unwrap();
        let mut versions = Vec::new();
        for _ in 0..2 {
            let response = http
                .get(format!("http://127.0.0.1:{port}/h2"))
                .send()
                .await
                .unwrap();
            versions.push((response.version(), response.text().await.unwrap()));
        }
        versions
    });
    for _ in 0..2 {
        let request = accept(server);
        respond(Action::Respond, request, b"over h2");
        super::release(request);
    }
    for (version, body) in client.join().unwrap() {
        assert_eq!(version, reqwest::Version::HTTP_2);
        assert_eq!(body, "over h2");
    }
    close(server);
}
#[test]
fn concurrency_limit_answers_503() {
    let (server, port) = listen_with(Settings {
        limit_concurrency: 1,
        ..Settings::default()
    });
    let first = client(async move {
        reqwest::get(format!("http://127.0.0.1:{port}/slow"))
            .await
            .unwrap()
            .status()
            .as_u16()
    });
    let request = accept(server);
    let second = client(async move {
        reqwest::get(format!("http://127.0.0.1:{port}/fast"))
            .await
            .unwrap()
            .status()
            .as_u16()
    });
    assert_eq!(second.join().unwrap(), 503);
    respond(Action::Respond, request, b"");
    super::release(request);
    assert_eq!(first.join().unwrap(), 200);
    close(server);
}
#[test]
fn oversized_headers_and_idle_connections_are_closed() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (server, port) = listen_with(Settings {
        max_header_bytes: 8192,
        keep_alive_ms: 200,
        ..Settings::default()
    });
    let probe = client(async move {
        let mut big = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = format!(
            "GET / HTTP/1.1\r\nHost: x\r\nX-Big: {}\r\n\r\n",
            "a".repeat(20000)
        );
        let _ = big.write_all(request.as_bytes()).await;
        let mut reply = Vec::new();
        let _ = big.read_to_end(&mut reply).await;
        let rejected =
            reply.is_empty() || String::from_utf8_lossy(&reply).starts_with("HTTP/1.1 431");
        let mut idle = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let started = Instant::now();
        let mut byte = [0u8; 1];
        let closed = tokio::time::timeout(Duration::from_secs(5), idle.read(&mut byte)).await;
        (
            rejected,
            matches!(closed, Ok(Ok(0)) | Ok(Err(_))),
            started.elapsed(),
        )
    });
    let (rejected, closed, elapsed) = probe.join().unwrap();
    assert!(rejected);
    assert!(closed);
    assert!(elapsed < Duration::from_secs(5));
    close(server);
}
#[test]
fn graceful_shutdown_waits_and_times_out() {
    for (graceful, expect_ok) in [(0, true), (200, false)] {
        let (server, port) = listen_with(Settings {
            graceful_ms: graceful,
            ..Settings::default()
        });
        let pending = client(async move {
            reqwest::get(format!("http://127.0.0.1:{port}/"))
                .await
                .map(|r| r.status().as_u16())
                .is_ok()
        });
        let request = accept(server);
        let waiting = op(Action::Wait, server, vec![]);
        take(op(Action::Shutdown, server, vec![]));
        if expect_ok {
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(operations::OPERATIONS.result(waiting)[0], 0);
            respond(Action::Respond, request, b"done");
        }
        take(waiting);
        assert_eq!(pending.join().unwrap(), expect_ok);
        super::release(request);
        close(server);
    }
}
#[test]
fn server_websocket_limits_and_shutdown_close() {
    let (server, port) = listen_with(Settings {
        ws_max_size: 16,
        ..Settings::default()
    });
    let mut sockets = Vec::new();
    for _ in 0..2 {
        let connecting = connect(port);
        let request = accept(server);
        let server_socket = handle(take(op(Action::Upgrade, request, vec![])));
        let client_socket = handle(take(connecting));
        sockets.push((request, server_socket, client_socket));
    }
    let (_, big_server, big_client) = sockets[0];
    take(ws(Ws::SendBinary, big_client, vec![0; 64]));
    let (kind, _, code) = event(take(ws(Ws::Receive, big_server, vec![])));
    assert_eq!((kind, code), (3, 1009));
    let (_, _, restart_client) = sockets[1];
    take(op(Action::Shutdown, server, vec![]));
    let (kind, _, code) = event(take(ws(Ws::Receive, restart_client, vec![])));
    assert_eq!((kind, code), (3, 1012));
    for (request, server_socket, client_socket) in sockets {
        websocket::close(client_socket);
        websocket::close(server_socket);
        super::release(request);
    }
    close(server);
}
#[test]
fn tls_valid_certificate_and_invalid_configuration() {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let cert_path = directory.path().join("cert.pem");
    let key_path = directory.path().join("key.pem");
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.signing_key.serialize_pem()).unwrap();
    let (server, port) = listen(cert_path.to_str().unwrap(), key_path.to_str().unwrap());
    let client = std::thread::spawn(move || {
        operations::runtime().unwrap().block_on(async move {
            let client = reqwest::Client::builder()
                .tls_certs_only([reqwest::Certificate::from_der(cert.cert.der()).unwrap()])
                .build()
                .unwrap();
            let response = client
                .get(format!("https://localhost:{port}/secure"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "secure");
        })
    });
    let request = accept(server);
    respond(Action::Respond, request, b"secure");
    super::release(request);
    client.join().unwrap();
    close(server);
    let invalid = listen_operation(Settings {
        cert: cert_path.to_str().unwrap(),
        ..Settings::default()
    });
    assert_eq!(failure(invalid), "EINVAL");
}
#[test]
fn websocket_upgrade_echo_close_and_handshake_validation() {
    let (server, port) = listen("", "");
    let connecting = connect(port);
    let request = accept(server);
    let server_socket = handle(take(op(Action::Upgrade, request, vec![])));
    let client_socket = handle(take(connecting));
    take(ws(Ws::SendText, client_socket, b"hello".to_vec()));
    assert_eq!(
        event(take(ws(Ws::Receive, server_socket, vec![]))),
        (1, b"hello".to_vec(), 0)
    );
    take(ws(Ws::SendBinary, server_socket, vec![0, 127, 255]));
    assert_eq!(
        event(take(ws(Ws::Receive, client_socket, vec![]))),
        (2, vec![0, 127, 255], 0)
    );
    take(close_socket(client_socket, 1000, "done"));
    assert_eq!(
        event(take(ws(Ws::Receive, server_socket, vec![]))),
        (3, b"done".to_vec(), 1000)
    );
    take(op(Action::Disconnect, request, vec![]));
    websocket::close(client_socket);
    websocket::close(server_socket);
    super::release(request);
    let ordinary = std::thread::spawn(move || {
        operations::runtime().unwrap().block_on(async move {
            reqwest::get(format!("http://127.0.0.1:{port}/ws"))
                .await
                .unwrap()
                .status()
        })
    });
    let request = accept(server);
    let operation = op(Action::Upgrade, request, vec![]);
    let wire = wait(operation);
    assert_eq!(wire[0], 2);
    operations::OPERATIONS.release(operation);
    respond(Action::Respond, request, b"not a WebSocket");
    assert_eq!(ordinary.join().unwrap(), 200);
    super::release(request);
    close(server);
}
#[test]
fn cancellation_and_unclaimed_resource_release() {
    let (server, _) = listen("", "");
    let operation = op(Action::Accept, server, vec![]);
    operations::OPERATIONS.cancel(operation);
    assert_eq!(failure(operation), "ECANCELLED");
    let waiting = op(Action::Wait, server, vec![]);
    take(op(Action::Shutdown, server, vec![]));
    take(waiting);
    close(server);
}
