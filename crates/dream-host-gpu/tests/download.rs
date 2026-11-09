#[path = "../build/download.rs"]
pub mod download;

use download::{Pin, download};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::{Shutdown, TcpListener};
use std::path::Path;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tiny_http::{Response, Server, StatusCode};

const CONTENT: &[u8] = b"pinned GPU archive contents";

fn server(responses: Vec<(u16, &'static [u8], usize)>) -> (Pin, JoinHandle<()>) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let pin = Pin {
        url: format!("http://{}/archive.zip", server.server_addr()),
        sha256: format!("{:x}", Sha256::digest(CONTENT)),
    };
    let worker = thread::spawn(move || {
        for (status, body, length) in responses {
            let request = server
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .expect("expected download request");
            let response = Response::new(
                StatusCode(status),
                vec![],
                Cursor::new(body),
                Some(length),
                None,
            );
            request.respond(response).unwrap();
        }
    });
    (pin, worker)
}

fn success() -> (u16, &'static [u8], usize) {
    (200, CONTENT, CONTENT.len())
}

fn assert_no_partial(cache: &Path) {
    assert!(!cache.join("downloads/archive.part").exists());
}

#[test]
fn gateway_failures_retry_and_verified_cache_needs_no_network() {
    let cache = tempfile::tempdir().unwrap();
    let (pin, worker) = server(vec![(504, b"timeout", 7), (503, b"busy", 4), success()]);
    let archive = download(cache.path(), &pin, None);
    worker.join().unwrap();
    assert_eq!(std::fs::read(&archive).unwrap(), CONTENT);
    assert_eq!(download(cache.path(), &pin, None), archive);
    assert_no_partial(cache.path());
}

#[test]
fn truncated_transfers_restart_without_appending_to_the_partial_file() {
    let cache = tempfile::tempdir().unwrap();
    // A malformed body must close the socket; tiny_http keeps a short response alive.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let pin = Pin {
        url: format!("http://{}/archive.zip", listener.local_addr().unwrap()),
        sha256: format!("{:x}", Sha256::digest(CONTENT)),
    };
    let worker = thread::spawn(move || {
        for body in [b"partial" as &[u8], CONTENT] {
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    other => panic!("expected download connection: {other:?}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = BufReader::new(&mut stream);
            loop {
                let mut line = String::new();
                assert!(request.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                CONTENT.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            stream.shutdown(Shutdown::Both).unwrap();
        }
    });
    let archive = download(cache.path(), &pin, None);
    worker.join().unwrap();
    assert_eq!(std::fs::read(archive).unwrap(), CONTENT);
    assert_no_partial(cache.path());
}

#[test]
fn a_corrupt_cached_archive_can_be_replaced_on_windows() {
    let cache = tempfile::tempdir().unwrap();
    std::fs::create_dir(cache.path().join("downloads")).unwrap();
    std::fs::write(cache.path().join("downloads/archive.zip"), b"corrupt").unwrap();
    let (pin, worker) = server(vec![success()]);
    let archive = download(cache.path(), &pin, None);
    worker.join().unwrap();
    assert_eq!(std::fs::read(archive).unwrap(), CONTENT);
    assert_no_partial(cache.path());
}

#[test]
fn permanent_http_errors_and_hash_mismatches_do_not_retry_or_enter_the_cache() {
    for response in [(404, b"missing" as &'static [u8], 7), (200, b"tampered", 8)] {
        let cache = tempfile::tempdir().unwrap();
        let (pin, worker) = server(vec![response]);
        let error = std::panic::catch_unwind(|| download(cache.path(), &pin, None)).unwrap_err();
        let message = error.downcast_ref::<String>().unwrap();
        assert!(message.contains(if response.0 == 404 {
            "after 1 attempt(s)"
        } else {
            "SHA-256 mismatch"
        }));
        worker.join().unwrap();
        assert!(!cache.path().join("downloads/archive.zip").exists());
        assert_no_partial(cache.path());
    }
}

#[test]
fn persistent_transient_errors_stop_after_four_attempts() {
    let cache = tempfile::tempdir().unwrap();
    let (pin, worker) = server(vec![(504, b"timeout" as &'static [u8], 7); 4]);
    let error = std::panic::catch_unwind(|| download(cache.path(), &pin, None)).unwrap_err();
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("after 4 attempt(s)")
    );
    worker.join().unwrap();
    assert!(!cache.path().join("downloads/archive.zip").exists());
    assert_no_partial(cache.path());
}
