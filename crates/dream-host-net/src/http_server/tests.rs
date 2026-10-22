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
fn listen(cert: &str, key: &str) -> (i32, u16) {
    let meta = metadata(take(start(json!({"action":"listen","host":"127.0.0.1","port":0,"cert":cert,"key":key,"timeout_ms":5000}).to_string(), vec![])));
    (
        meta["id"].as_i64().unwrap() as i32,
        meta["port"].as_u64().unwrap() as u16,
    )
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
    let parts = metadata(take(op("multipart", request, vec![])));
    assert_eq!(parts[0]["name"], "title");
    assert_eq!(parts[0]["bytes"], hex::encode("hello"));
    assert_eq!(parts[1]["filename"], "a.txt");
    assert_eq!(parts[1]["bytes"], hex::encode("file data"));
    respond("stream_begin", request, b"");
    take(op("stream_write", request, b"one".to_vec()));
    take(op("stream_write", request, b"two".to_vec()));
    take(op("stream_end", request, vec![]));
    client.join().unwrap();
    super::release(request);
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
    let wire = wait(start(json!({"action":"listen","host":"127.0.0.1","port":0,"cert":cert_path,"key":"","timeout_ms":5000}).to_string(), vec![]));
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
