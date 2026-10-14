#![cfg(feature = "native")]
#[path = "common/http_fixture.rs"]
mod fixture;
use dream::driver::{
    test::{TestHost, TestOptions, run_tests},
    toolchain::ToolchainConfig,
};
use std::sync::{Arc, atomic::Ordering};

#[test]
fn http_client_streams_and_cancels_on_native_and_node() {
    let server = fixture::HttpFixture::start();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("dream.toml"),
        "[package]\nname = \"http_client\"\nversion = \"0.1.0\"\ntype = \"test\"\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("main.dream"),
        include_str!("fixtures/compiler/http_client.dream").replace("__BASE__", &server.url),
    )
    .unwrap();
    let result = run_tests(
        &Arc::new(ToolchainConfig::default()),
        root.path(),
        &TestOptions {
            hosts: vec![TestHost::Native, TestHost::Node],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(result.ok(), "{result:?}");
    assert!(server.accepted.load(Ordering::Relaxed) > 0);
}
