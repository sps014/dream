use super::{close, start};
use crate::{socket_operations as operations, websocket};
use serde_json::{Value, json};
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
fn op(action: &str, id: i32, bytes: Vec<u8>) -> i32 {
    start(
        json!({"action":action,"id":id,"timeout_ms":5000}).to_string(),
        bytes,
    )
}
fn config(overrides: Value) -> Value {
    let mut config = json!({
        "ssl_certfile":"","ssl_keyfile":"","limit_concurrency":0,"backlog":2048,
        "timeout_keep_alive_ms":5000,"timeout_graceful_shutdown_ms":0,"max_header_bytes":16384,
        "ws_max_size":16777216,"ws_max_queue":32,"ws_ping_interval_ms":20000,"ws_ping_timeout_ms":20000
    });
    for (key, value) in overrides.as_object().unwrap() {
        config[key] = value.clone();
    }
    config
}
fn listen_with(overrides: Value) -> (i32, u16) {
    let meta = metadata(take(start(json!({"action":"listen","host":"127.0.0.1","port":0,"config":config(overrides),"timeout_ms":5000}).to_string(), vec![])));
    (
        meta["id"].as_i64().unwrap() as i32,
        meta["port"].as_u64().unwrap() as u16,
    )
}
fn listen(cert: &str, key: &str) -> (i32, u16) {
    listen_with(json!({"ssl_certfile":cert,"ssl_keyfile":key}))
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
    serde_json::from_slice::<Value>(&wire[1..]).unwrap()["code"]
        .as_str()
        .unwrap()
        .to_string()
}
fn read_chunk(request: i32, max: u64) -> Vec<u8> {
    take(start(
        json!({"action":"body_read","id":request,"max_bytes":max,"timeout_ms":5000}).to_string(),
        vec![],
    ))
}
fn form(request: i32, limits: Value) -> i32 {
    let mut value = json!({"action":"form","id":request,"timeout_ms":5000,"max_files":1000,"max_fields":1000,"max_part_size":1048576});
    for (key, limit) in limits.as_object().unwrap() {
        value[key] = limit.clone();
    }
    start(value.to_string(), vec![])
}
fn accept(server: i32) -> i32 {
    metadata(take(op("accept", server, vec![])))["id"]
        .as_i64()
        .unwrap() as i32
}
fn respond(action: &str, id: i32, bytes: &[u8]) {
    take(start(json!({"action":action,"id":id,"status":200,"headers":[["x-repeat","a"],["x-repeat","b"]],"timeout_ms":5000}).to_string(), bytes.to_vec()));
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
    let parts = metadata(take(form(request, json!({}))));
    assert_eq!(parts["fields"][0]["name"], "title");
    assert_eq!(parts["fields"][0]["value"], "hello");
    let file = &parts["files"][0];
    assert_eq!(file["filename"], "a.txt");
    assert_eq!(file["size"], 9);
    let upload = file["handle"].as_i64().unwrap() as i32;
    let read = |max: i64| {
        take(start(
            json!({"action":"upload_read","id":upload,"max_bytes":max,"timeout_ms":5000})
                .to_string(),
            vec![],
        ))
    };
    assert_eq!(read(4), b"file");
    assert_eq!(read(-1), b" data");
    take(start(
        json!({"action":"upload_seek","id":upload,"offset":5,"timeout_ms":5000}).to_string(),
        vec![],
    ));
    assert_eq!(read(-1), b"data");
    respond("stream_begin", request, b"");
    take(op("stream_write", request, b"one".to_vec()));
    take(op("stream_write", request, b"two".to_vec()));
    take(op("stream_end", request, vec![]));
    client.join().unwrap();
    super::release(request);
    assert_eq!(
        failure(start(
            json!({"action":"upload_read","id":upload,"max_bytes":1,"timeout_ms":5000})
                .to_string(),
            vec![],
        )),
        "ECLOSED"
    );
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
        let second = http.post(&url).body("streamed").send().await.unwrap().status();
        (first, second)
    });
    let request = accept(server);
    assert_eq!(take(op("read_body", request, vec![])).len(), large.len());
    assert_eq!(read_chunk(request, 1 << 30).len(), large.len());
    assert!(read_chunk(request, 16).is_empty());
    assert_eq!(take(op("read_body", request, vec![])).len(), large.len());
    respond("respond", request, b"");
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
    assert_eq!(failure(op("read_body", request, vec![])), "ECLOSED");
    respond("respond", request, b"");
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
        format!("--b\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}.bin\"\r\n\r\nx\r\n")
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
    for limits in [
        json!({"max_fields":1}),
        json!({"max_files":1}),
        json!({"max_part_size":16}),
    ] {
        let request = accept(server);
        assert_eq!(failure(form(request, limits)), "EINVAL");
        respond("respond", request, b"");
        super::release(request);
    }
    poster.join().unwrap();
    close(server);
}
#[test]
fn large_file_parts_spool_to_disk_until_release() {
    let (server, port) = listen("", "");
    let poster = client(async move {
        let mut body = b"--b\r\nContent-Disposition: form-data; name=\"f\"; filename=\"big.bin\"\r\n\r\n".to_vec();
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
    let parts = metadata(take(form(request, json!({}))));
    assert_eq!(parts["files"][0]["size"], 3 << 20);
    let handle = parts["files"][0]["handle"].as_i64().unwrap() as i32;
    assert!(super::state::lock(&super::state::upload(handle).unwrap()).is_rolled());
    respond("respond", request, b"");
    super::release(request);
    assert!(super::state::upload(handle).is_err());
    assert_eq!(poster.join().unwrap(), 200);
    close(server);
}
#[test]
fn websocket_ping_timeout_closes_silent_peer() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (server, port) = listen_with(json!({"ws_ping_interval_ms":50,"ws_ping_timeout_ms":100}));
    let peer = client(async move {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(b"GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").await.unwrap();
        let mut sink = vec![0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_secs(3), async {
            while stream.read(&mut sink).await.is_ok_and(|n| n > 0) {}
        })
        .await;
    });
    let request = accept(server);
    let socket = metadata(take(op("upgrade", request, vec![])))["id"]
        .as_i64()
        .unwrap() as i32;
    let message = metadata(take(websocket::start(
        json!({"action":"receive","id":socket,"timeout_ms":5000}).to_string(),
        vec![],
    )));
    assert_eq!(message["code"], 1011);
    websocket::close(socket);
    super::release(request);
    peer.join().unwrap();
    close(server);
}
#[test]
fn concurrency_limit_answers_503() {
    let (server, port) = listen_with(json!({"limit_concurrency":1}));
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
    respond("respond", request, b"");
    super::release(request);
    assert_eq!(first.join().unwrap(), 200);
    close(server);
}
#[test]
fn oversized_headers_and_idle_connections_are_closed() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (server, port) = listen_with(json!({"max_header_bytes":8192,"timeout_keep_alive_ms":200}));
    let probe = client(async move {
        let mut big = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let request = format!("GET / HTTP/1.1\r\nHost: x\r\nX-Big: {}\r\n\r\n", "a".repeat(20000));
        let _ = big.write_all(request.as_bytes()).await;
        let mut reply = Vec::new();
        let _ = big.read_to_end(&mut reply).await;
        let rejected = reply.is_empty() || String::from_utf8_lossy(&reply).starts_with("HTTP/1.1 431");
        let mut idle = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let started = Instant::now();
        let mut byte = [0u8; 1];
        let closed = tokio::time::timeout(Duration::from_secs(5), idle.read(&mut byte)).await;
        (rejected, matches!(closed, Ok(Ok(0)) | Ok(Err(_))), started.elapsed())
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
        let (server, port) = listen_with(json!({"timeout_graceful_shutdown_ms":graceful}));
        let pending = client(async move {
            reqwest::get(format!("http://127.0.0.1:{port}/"))
                .await
                .map(|r| r.status().as_u16())
                .is_ok()
        });
        let request = accept(server);
        let waiting = op("wait", server, vec![]);
        take(op("shutdown", server, vec![]));
        if expect_ok {
            std::thread::sleep(Duration::from_millis(100));
            assert_eq!(operations::OPERATIONS.result(waiting)[0], 0);
            respond("respond", request, b"done");
        }
        take(waiting);
        assert_eq!(pending.join().unwrap(), expect_ok);
        super::release(request);
        close(server);
    }
}
#[test]
fn server_websocket_limits_and_shutdown_close() {
    let (server, port) = listen_with(json!({"ws_max_size":16}));
    let connect = |port: u16| {
        websocket::start(
            json!({"action":"connect","url":format!("ws://127.0.0.1:{port}/ws"),"timeout_ms":5000})
                .to_string(),
            vec![],
        )
    };
    let ws = |action: &str, id: i32, bytes: Vec<u8>| {
        websocket::start(
            json!({"action":action,"id":id,"timeout_ms":5000}).to_string(),
            bytes,
        )
    };
    let mut sockets = Vec::new();
    for _ in 0..2 {
        let connecting = connect(port);
        let request = accept(server);
        let server_socket = metadata(take(op("upgrade", request, vec![])))["id"]
            .as_i64()
            .unwrap() as i32;
        let client_socket = metadata(take(connecting))["id"].as_i64().unwrap() as i32;
        sockets.push((request, server_socket, client_socket));
    }
    let (_, big_server, big_client) = sockets[0];
    take(ws("send_binary", big_client, vec![0; 64]));
    let message = metadata(take(ws("receive", big_server, vec![])));
    assert_eq!((message["kind"].as_str(), message["code"].as_u64()), (Some("close"), Some(1009)));
    let (_, _, restart_client) = sockets[1];
    take(op("shutdown", server, vec![]));
    let message = metadata(take(ws("receive", restart_client, vec![])));
    assert_eq!((message["kind"].as_str(), message["code"].as_u64()), (Some("close"), Some(1012)));
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
    respond("respond", request, b"secure");
    super::release(request);
    client.join().unwrap();
    close(server);
    let wire = wait(start(json!({"action":"listen","host":"127.0.0.1","port":0,"config":config(json!({"ssl_certfile":cert_path})),"timeout_ms":5000}).to_string(), vec![]));
    assert_eq!(wire[0], 2);
    assert_eq!(
        serde_json::from_slice::<Value>(&wire[1..]).unwrap()["code"],
        "EINVAL"
    );
}
#[test]
fn websocket_upgrade_echo_close_and_handshake_validation() {
    let (server, port) = listen("", "");
    let connecting = websocket::start(
        json!({"action":"connect","url":format!("ws://127.0.0.1:{port}/ws"),"timeout_ms":5000})
            .to_string(),
        vec![],
    );
    let request = accept(server);
    let server_socket = metadata(take(op("upgrade", request, vec![])))["id"]
        .as_i64()
        .unwrap() as i32;
    let client_socket = metadata(take(connecting))["id"].as_i64().unwrap() as i32;
    let ws = |action: &str, id, bytes| {
        websocket::start(
            json!({"action":action,"id":id,"code":1000,"reason":"done","timeout_ms":5000})
                .to_string(),
            bytes,
        )
    };
    take(ws("send_text", client_socket, b"hello".to_vec()));
    assert_eq!(
        metadata(take(ws("receive", server_socket, vec![])))["text"],
        "hello"
    );
    take(ws("send_binary", server_socket, vec![0, 127, 255]));
    assert_eq!(
        metadata(take(ws("receive", client_socket, vec![])))["bytes"],
        json!([0, 127, 255])
    );
    take(ws("close", client_socket, vec![]));
    let message = metadata(take(ws("receive", server_socket, vec![])));
    assert_eq!(message["kind"], "close");
    assert_eq!(message["code"], 1000);
    take(op("disconnect", request, vec![]));
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
    let operation = op("upgrade", request, vec![]);
    let wire = wait(operation);
    assert_eq!(wire[0], 2);
    operations::OPERATIONS.release(operation);
    respond("respond", request, b"not a WebSocket");
    assert_eq!(ordinary.join().unwrap(), 200);
    super::release(request);
    close(server);
}
#[test]
fn cancellation_and_unclaimed_resource_release() {
    let (server, _) = listen("", "");
    let operation = op("accept", server, vec![]);
    operations::OPERATIONS.cancel(operation);
    let wire = wait(operation);
    assert_eq!(wire[0], 2);
    assert_eq!(
        serde_json::from_slice::<Value>(&wire[1..]).unwrap()["code"],
        "ECANCELLED"
    );
    operations::OPERATIONS.release(operation);
    let waiting = op("wait", server, vec![]);
    take(op("shutdown", server, vec![]));
    take(waiting);
    close(server);
}
