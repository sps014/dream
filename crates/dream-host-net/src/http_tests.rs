use super as http;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
#[path = "../../../tests/common/http_fixture.rs"]
mod fixture;

fn wait(id: i32) -> Vec<u8> {
    assert!(id > 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let wire = http::poll(id);
        if wire[0] != 0 {
            return wire;
        }
        assert!(
            Instant::now() < deadline,
            "HTTP operation timed out in harness"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn take(id: i32) -> Vec<u8> {
    let wire = wait(id);
    if wire[0] == 1 {
        http::claim(id);
    }
    http::release(id);
    wire
}
fn json(wire: &[u8]) -> serde_json::Value {
    serde_json::from_slice(&wire[1..]).unwrap()
}
fn request(
    client: i32,
    url: &str,
    method: &str,
    timeout: u64,
    redirects: i32,
    bytes: Vec<u8>,
) -> i32 {
    http::send(client, &serde_json::json!({"url":url,"method":method,"headers":[["x-repeat","a"],["x-repeat","b"]],"timeout_ms":timeout,"connect_timeout_ms":1000,"max_redirects":redirects}).to_string(), bytes)
}
#[test]
fn pooled_http_transport_cancellation_streaming_deadlines_and_tls_rejection() {
    let server = fixture::HttpFixture::start();
    let client = http::client_create();
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let wire = take(request(
            client,
            &format!("{}/echo", server.url),
            method,
            2000,
            5,
            b"hello".to_vec(),
        ));
        assert_eq!(wire[0], 1, "{}", String::from_utf8_lossy(&wire));
        let metadata = json(&wire);
        assert_eq!(metadata["status"], 404);
        let body = metadata["body"].as_i64().unwrap() as i32;
        assert_eq!(take(http::read(body, 2)), b"\x01he");
        assert_eq!(take(http::read(body, 100)), b"\x01llo");
        assert_eq!(take(http::read(body, 1)), [1]);
        http::body_close(body);
    }
    assert!(
        server.accepted.load(Ordering::Relaxed) < 4,
        "HTTP connections were not reused: {}",
        server.accepted.load(Ordering::Relaxed)
    );
    let wire = take(request(
        client,
        &format!("{}/redirect", server.url),
        "GET",
        2000,
        5,
        vec![],
    ));
    let metadata = json(&wire);
    assert_eq!(metadata["status"], 200);
    assert!(metadata["url"].as_str().unwrap().ends_with("/stream"));
    assert_eq!(
        metadata["headers"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|pair| pair[0] == "set-cookie")
            .count(),
        2
    );
    let body = metadata["body"].as_i64().unwrap() as i32;
    let mut output = Vec::<u8>::new();
    loop {
        let chunk = take(http::read(body, 2));
        assert_eq!(chunk[0], 1);
        if chunk.len() == 1 {
            break;
        }
        output.extend_from_slice(&chunk[1..]);
    }
    assert_eq!(output, b"firstsecond");
    http::body_close(body);
    for (path, timeout, redirects, code) in [
        ("/slow", 10, 5, "ETIMEOUT"),
        ("/loop", 1000, 1, "EREDIRECT"),
    ] {
        let wire = take(request(
            client,
            &format!("{}{path}", server.url),
            "GET",
            timeout,
            redirects,
            vec![],
        ));
        assert_eq!(wire[0], 2);
        assert_eq!(json(&wire)["code"], code);
    }
    let orphan = request(client, &server.url, "GET", 1000, 5, vec![]);
    let wire = wait(orphan);
    let body = json(&wire)["body"].as_i64().unwrap() as i32;
    http::release(orphan);
    assert_eq!(json(&take(http::read(body, 1)))["code"], "ECLOSED");
    let pending = request(
        client,
        &format!("{}/slow", server.url),
        "GET",
        1000,
        5,
        vec![],
    );
    http::client_close(client);
    assert_eq!(json(&take(pending))["code"], "ECLOSED");
    let client = http::client_create();
    let wire = take(request(
        client,
        &format!("{}/slow-body", server.url),
        "GET",
        30,
        5,
        vec![],
    ));
    let body = json(&wire)["body"].as_i64().unwrap() as i32;
    assert_eq!(take(http::read(body, 100)), b"\x01first");
    assert_eq!(json(&take(http::read(body, 100)))["code"], "ETIMEOUT");
    http::body_close(body);
    let wire = take(request(
        client,
        &format!("{}/disconnect", server.url),
        "GET",
        1000,
        5,
        vec![],
    ));
    let body = json(&wire)["body"].as_i64().unwrap() as i32;
    let first = take(http::read(body, 100));
    let error = if first[0] == 1 {
        take(http::read(body, 100))
    } else {
        first
    };
    assert_eq!(error[0], 2);
    http::body_close(body);
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let certificate = certified.cert.der().clone();
    // Workspace dependencies can enable both Rustls providers; the fixture must choose one.
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::aws_lc_rs::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![certified.cert.der().clone()],
        rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
            certified.signing_key.serialize_der(),
        )),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let tls = std::thread::spawn(move || {
        let config = Arc::new(config);
        for _ in 0..2 {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let connection = rustls::ServerConnection::new(config.clone()).unwrap();
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                if stream.read_exact(&mut byte).is_err() {
                    break;
                }
                headers.push(byte[0]);
            }
            if headers.ends_with(b"\r\n\r\n") {
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello");
            }
        }
    });
    let url = format!("https://localhost:{}", address.port());
    // Controlled trust stores avoid OS certificate lookup/network activity in this local fixture.
    let resource = http::lock(http::registry()).clients[&client].clone();
    let untrusted = {
        let _guard = crate::socket_operations::runtime().unwrap().enter();
        reqwest::Client::builder()
            .tls_certs_only([])
            .build()
            .unwrap()
    };
    http::lock(&resource.pools).insert((1000, 5), untrusted);
    let rejected = take(request(client, &url, "GET", 2000, 5, vec![]));
    assert_eq!(rejected[0], 2);
    assert_eq!(json(&rejected)["code"], "ETRANSPORT");
    let trusted = {
        let _guard = crate::socket_operations::runtime().unwrap().enter();
        reqwest::Client::builder()
            .tls_certs_only([reqwest::Certificate::from_der(&certificate).unwrap()])
            .build()
            .unwrap()
    };
    http::lock(&resource.pools).insert((1000, 5), trusted);
    let wire = take(request(client, &url, "GET", 2000, 5, vec![]));
    assert_eq!(wire[0], 1, "{}", String::from_utf8_lossy(&wire));
    let body = json(&wire)["body"].as_i64().unwrap() as i32;
    assert_eq!(take(http::read(body, 100)), b"\x01hello");
    http::body_close(body);
    tls.join().unwrap();
    http::client_close(client);
    let resources = http::lock(http::registry());
    assert!(resources.clients.is_empty());
    assert!(resources.bodies.is_empty());
    assert!(resources.operations.is_empty());
}
