#![cfg(feature = "native")]
use dream::driver::{
    test::{TestHost, TestOptions, run_tests},
    toolchain::ToolchainConfig,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

#[test]
fn websocket_client_echo_and_cancellation_on_native_and_node() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("ws://{}/echo", listener.local_addr().unwrap());
    let (stop, mut stopped) = tokio::sync::watch::channel(false);
    let server = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = stopped.changed() => break,
                    result = listener.accept() => {
                        let (socket, _) = result.unwrap();
                        connections.spawn(async move {
                            let Ok(mut socket) = tokio_tungstenite::accept_async(socket).await else { return; };
                            while let Some(Ok(message)) = socket.next().await {
                                if message.is_close() { let _ = socket.flush().await; break; }
                                if socket.send(message).await.is_err() { break; }
                            }
                        });
                    }
                    Some(_) = connections.join_next(), if !connections.is_empty() => {}
                }
            }
            connections.abort_all(); while connections.join_next().await.is_some() {}
        });
    });
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("dream.toml"),
        "[package]\nname = \"websocket-client\"\nversion = \"0.1.0\"\ntype = \"test\"\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("main.dream"),
        include_str!("fixtures/compiler/websocket_client.dream").replace("__URL__", &url),
    )
    .unwrap();
    let result = run_tests(
        &Arc::new(ToolchainConfig::default()),
        root.path(),
        &TestOptions {
            hosts: vec![TestHost::Native, TestHost::Node],
            ..Default::default()
        },
    );
    stop.send_replace(true);
    server.join().unwrap();
    assert!(
        result.as_ref().is_ok_and(|result| result.ok()),
        "{result:?}"
    );
}

#[test]
fn webapi_tls_listen_with_generated_certificate() {
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let root = tempfile::tempdir().unwrap();
    let cert = root.path().join("cert.pem");
    let key = root.path().join("key.pem");
    std::fs::write(&cert, certificate.cert.pem()).unwrap();
    std::fs::write(&key, certificate.signing_key.serialize_pem()).unwrap();
    std::fs::write(
        root.path().join("dream.toml"),
        r#"[package]
name = "webapi-tls"
version = "0.1.0"
type = "test"
"#,
    )
    .unwrap();
    let source = format!(
        r#"
import system.webapi;
import system.testing;
@test
@timeout(5000)
async fun tls_listen(): void {{
    let config = ServerConfig(ssl_certfile: {cert}, ssl_keyfile: {key});
    let app = WebApp();
    Assert.is_true(app.listen("127.0.0.1", 0, config).await.unwrap() > 0);
    app.shutdown(); app.wait().await;
    config.ssl_keyfile = "";
    Assert.is_true(WebApp().listen("127.0.0.1", 0, config).await.is_err());
}}
"#,
        cert = serde_json::to_string(&cert).unwrap(),
        key = serde_json::to_string(&key).unwrap()
    );
    std::fs::write(root.path().join("main.dream"), source).unwrap();
    let result = run_tests(
        &Arc::new(ToolchainConfig::default()),
        root.path(),
        &TestOptions {
            hosts: vec![TestHost::Native],
            ..Default::default()
        },
    );
    assert!(
        result.as_ref().is_ok_and(|result| result.ok()),
        "{result:?}"
    );
}
