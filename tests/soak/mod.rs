//! Long-uptime qualification for the web server, `Task` and WebSockets. Ignored by default:
//! it runs for an hour unless `DREAM_SOAK_MINUTES` says otherwise.
//!
//! ```text
//! cargo test --test integration soak:: -- --ignored --nocapture
//! ```
//!
//! Each sampling interval puts the service under load (keep-alive and fresh connections, bodies
//! past the heap's large-block size, uploads, streaming, `Task.spawn` and WebSocket churn) and then
//! leaves it idle before sampling, so the samples show what the process keeps between bursts.
//! `DREAM_SOAK_WARMUP_MINUTES` (default 10) and `DREAM_SOAK_SAMPLE_SECS` (default 60) tune it.
#![cfg(feature = "native")]

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;

type Failure = Box<dyn std::error::Error + Send + Sync>;

/// RSS growth allowed across the post-warm-up window, in MB per hour.
const RSS_SLOPE_LIMIT_MB_PER_HOUR: f64 = 1.0;
/// Below this post-warm-up window the RSS slope is reported but too noisy to judge.
const RSS_SLOPE_MIN_WINDOW: Duration = Duration::from_secs(30 * 60);
const LIVE_BYTES_SLACK: i64 = 1 << 20;
const THREAD_SLACK: u64 = 4;
const HANDLE_SLACK: u64 = 16;
/// Each closed client connection holds an ephemeral port in TIME_WAIT for up to two minutes, so
/// unpaced fresh connections exhaust the client's port range (Windows allows about 16k).
const FRESH_CONNECTION_PACE: Duration = Duration::from_millis(50);

fn setting(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[derive(Default)]
struct Totals {
    requests: AtomicU64,
    websockets: AtomicU64,
    server_errors: AtomicU64,
    failures: AtomicU64,
}

impl Totals {
    fn record(&self, result: Result<u16, Failure>) {
        match result {
            Ok(status) if status >= 500 => {
                self.server_errors.fetch_add(1, Ordering::Relaxed);
                eprintln!("[soak] server error {status}");
            }
            Ok(_) => {
                self.requests.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                if self.failures.fetch_add(1, Ordering::Relaxed) < 10 {
                    eprintln!("[soak] request failed: {error}");
                }
            }
        }
    }
}

struct Sample {
    at: Duration,
    rss: u64,
    threads: Option<u64>,
    handles: Option<u64>,
    committed: i64,
    live: i64,
}

#[test]
#[ignore = "hour-long soak; run locally with `cargo test --test integration soak:: -- --ignored --nocapture`"]
fn web_task_and_websocket_soak() {
    let total = Duration::from_secs(60 * setting("DREAM_SOAK_MINUTES", 60));
    let warmup = Duration::from_secs(60 * setting("DREAM_SOAK_WARMUP_MINUTES", 10)).min(total / 2);
    let interval = Duration::from_secs(setting("DREAM_SOAK_SAMPLE_SECS", 60).max(4));

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("service.dream");
    std::fs::write(&source, include_str!("service.dream")).unwrap();
    let binary = dir.path().join("service.bin");
    let built = Command::new(env!("CARGO_BIN_EXE_dream"))
        .args(["build", "--release", "-o"])
        .arg(binary.with_extension("ll"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let mut service = Service::start(&binary);
    let addr = service.addr;
    eprintln!(
        "[soak] service on {addr}: {} min, {} min warm-up, sampled every {} s",
        total.as_secs() / 60,
        warmup.as_secs() / 60,
        interval.as_secs()
    );

    let totals = Arc::new(Totals::default());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let start = Instant::now();
    let mut samples = Vec::new();
    while start.elapsed() < total {
        let load = interval * 3 / 4;
        runtime.block_on(run_load(addr, Instant::now() + load, totals.clone()));
        std::thread::sleep(interval - load);
        let (committed, live) = runtime.block_on(heap_stats(addr)).unwrap();
        let sample = Sample {
            at: start.elapsed(),
            rss: service.rss(),
            threads: service.threads(),
            handles: service.handles(),
            committed,
            live,
        };
        eprintln!(
            "[soak {:>6.1}m] rss={} KiB threads={:?} handles={:?} heap committed={} KiB live={} KiB requests={} websockets={} 5xx={} failures={}",
            sample.at.as_secs_f64() / 60.0,
            sample.rss / 1024,
            sample.threads,
            sample.handles,
            sample.committed / 1024,
            sample.live / 1024,
            totals.requests.load(Ordering::Relaxed),
            totals.websockets.load(Ordering::Relaxed),
            totals.server_errors.load(Ordering::Relaxed),
            totals.failures.load(Ordering::Relaxed),
        );
        samples.push(sample);
        assert!(service.running(), "the service exited during the soak");
    }
    drop(service);

    assert_eq!(totals.server_errors.load(Ordering::Relaxed), 0, "5xx responses");
    assert_eq!(totals.failures.load(Ordering::Relaxed), 0, "failed requests");
    let settled: Vec<&Sample> = samples.iter().filter(|s| s.at >= warmup).collect();
    assert!(settled.len() >= 2, "too few samples after the warm-up");
    let first = settled[0];
    let last = settled[settled.len() - 1];
    let peak_live = settled.iter().map(|s| s.live).max().unwrap();
    assert!(
        peak_live - first.live <= LIVE_BYTES_SLACK,
        "idle live heap grew from {} to {} bytes",
        first.live,
        peak_live
    );
    if let (Some(before), Some(after)) = (first.threads, last.threads) {
        assert!(after <= before + THREAD_SLACK, "threads grew from {before} to {after}");
    }
    if let (Some(before), Some(after)) = (first.handles, last.handles) {
        assert!(after <= before + HANDLE_SLACK, "handles grew from {before} to {after}");
    }
    let slope = rss_slope_mb_per_hour(&settled);
    eprintln!("[soak] post-warm-up RSS slope {slope:.2} MB/h");
    if last.at - first.at >= RSS_SLOPE_MIN_WINDOW {
        assert!(
            slope < RSS_SLOPE_LIMIT_MB_PER_HOUR,
            "RSS grows {slope:.2} MB/h"
        );
    }
}

/// Least-squares slope of RSS over time.
fn rss_slope_mb_per_hour(samples: &[&Sample]) -> f64 {
    let n = samples.len() as f64;
    let hours: Vec<f64> = samples.iter().map(|s| s.at.as_secs_f64() / 3600.0).collect();
    let mb: Vec<f64> = samples.iter().map(|s| s.rss as f64 / 1e6).collect();
    let mean_h = hours.iter().sum::<f64>() / n;
    let mean_mb = mb.iter().sum::<f64>() / n;
    let covariance: f64 = hours.iter().zip(&mb).map(|(h, m)| (h - mean_h) * (m - mean_mb)).sum();
    let variance: f64 = hours.iter().map(|h| (h - mean_h).powi(2)).sum();
    if variance == 0.0 { 0.0 } else { covariance / variance }
}

async fn run_load(addr: SocketAddr, until: Instant, totals: Arc<Totals>) {
    let mut workers = tokio::task::JoinSet::new();
    for _ in 0..4 {
        workers.spawn(keep_alive_worker(addr, until, totals.clone()));
    }
    workers.spawn(fresh_connection_worker(addr, until, totals.clone()));
    workers.spawn(upload_worker(addr, until, totals.clone()));
    workers.spawn(websocket_worker(addr, until, totals.clone()));
    while workers.join_next().await.is_some() {}
}

async fn keep_alive_worker(addr: SocketAddr, until: Instant, totals: Arc<Totals>) {
    let small = Bytes::from(vec![b'x'; 4096]);
    let large = Bytes::from(vec![b'y'; 1 << 20]);
    while Instant::now() < until {
        let mut sender = match connect(addr).await {
            Ok(sender) => sender,
            Err(error) => {
                totals.record(Err(error));
                continue;
            }
        };
        for round in 0..64u32 {
            if Instant::now() >= until {
                break;
            }
            let body = if round % 16 == 0 { large.clone() } else { small.clone() };
            for (method, path, body) in [
                ("GET", "/plain", Bytes::new()),
                ("POST", "/echo", body),
                ("GET", "/stream", Bytes::new()),
                ("GET", "/task", Bytes::new()),
            ] {
                totals.record(send(&mut sender, method, path, "text/plain", body).await);
            }
        }
    }
}

async fn fresh_connection_worker(addr: SocketAddr, until: Instant, totals: Arc<Totals>) {
    while Instant::now() < until {
        let result = async {
            let mut sender = connect(addr).await?;
            send(&mut sender, "GET", "/plain", "text/plain", Bytes::new()).await
        };
        totals.record(result.await);
        tokio::time::sleep(FRESH_CONNECTION_PACE).await;
    }
}

async fn upload_worker(addr: SocketAddr, until: Instant, totals: Arc<Totals>) {
    let boundary = "dream-soak-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"data.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend(std::iter::repeat_n(7u8, 256 * 1024));
    body.extend(format!("\r\n--{boundary}--\r\n").into_bytes());
    let body = Bytes::from(body);
    let content_type = format!("multipart/form-data; boundary={boundary}");
    while Instant::now() < until {
        let result = async {
            let mut sender = connect(addr).await?;
            send(&mut sender, "POST", "/upload", &content_type, body.clone()).await
        };
        totals.record(result.await);
        tokio::time::sleep(FRESH_CONNECTION_PACE).await;
    }
}

async fn websocket_worker(addr: SocketAddr, until: Instant, totals: Arc<Totals>) {
    while Instant::now() < until {
        match websocket_round(addr).await {
            Ok(()) => {
                totals.websockets.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => totals.record(Err(error)),
        }
        tokio::time::sleep(FRESH_CONNECTION_PACE).await;
    }
}

async fn websocket_round(addr: SocketAddr) -> Result<(), Failure> {
    let stream = TcpStream::connect(addr).await?;
    let (mut socket, _) = tokio_tungstenite::client_async(format!("ws://{addr}/ws"), stream).await?;
    for i in 0..10 {
        let text = format!("message {i}");
        socket.send(Message::text(text.clone())).await?;
        match socket.next().await {
            Some(Ok(Message::Text(echo))) if echo.as_str() == text => {}
            other => return Err(format!("unexpected WebSocket reply {other:?}").into()),
        }
    }
    socket.send(Message::binary(vec![1u8, 2, 3])).await?;
    match socket.next().await {
        Some(Ok(Message::Binary(echo))) if echo.as_ref() == [1, 2, 3] => {}
        other => return Err(format!("unexpected WebSocket reply {other:?}").into()),
    }
    socket.close(None).await?;
    while let Some(message) = socket.next().await {
        if message.is_err() {
            break;
        }
    }
    Ok(())
}

async fn connect(addr: SocketAddr) -> Result<SendRequest<Full<Bytes>>, Failure> {
    let stream = TcpStream::connect(addr).await?;
    let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(connection);
    Ok(sender)
}

async fn send(
    sender: &mut SendRequest<Full<Bytes>>,
    method: &str,
    path: &str,
    content_type: &str,
    body: Bytes,
) -> Result<u16, Failure> {
    sender.ready().await?;
    let request = hyper::Request::builder()
        .method(method)
        .uri(path)
        .header(hyper::header::HOST, "127.0.0.1")
        .header(hyper::header::CONTENT_TYPE, content_type)
        .body(Full::new(body))?;
    let response = sender.send_request(request).await?;
    let status = response.status().as_u16();
    let body = response.into_body().collect().await?.to_bytes();
    if status >= 500 {
        eprintln!(
            "[soak] {method} {path} answered {status}: {}",
            String::from_utf8_lossy(&body[..body.len().min(200)])
        );
    }
    Ok(status)
}

async fn heap_stats(addr: SocketAddr) -> Result<(i64, i64), Failure> {
    let mut sender = connect(addr).await?;
    sender.ready().await?;
    let request = hyper::Request::builder()
        .uri("/stats")
        .header(hyper::header::HOST, "127.0.0.1")
        .body(Full::new(Bytes::new()))?;
    let body = sender.send_request(request).await?.into_body().collect().await?.to_bytes();
    let text = String::from_utf8(body.to_vec())?;
    let mut fields = text.split(' ').map(str::parse::<i64>);
    let committed = fields.next().ok_or("missing committed bytes")??;
    let live = fields.next().ok_or("missing live bytes")??;
    Ok((committed, live))
}

struct Service {
    child: Child,
    addr: SocketAddr,
}

impl Service {
    fn start(binary: &std::path::Path) -> Self {
        use std::io::BufRead;
        let mut child = Command::new(binary)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line
            .trim()
            .strip_prefix("listening ")
            .unwrap_or_else(|| panic!("service did not start: {line}"));
        let addr = format!("127.0.0.1:{port}").parse().unwrap();
        Service { child, addr }
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    #[cfg(target_os = "linux")]
    fn status_field(&self, name: &str) -> Option<u64> {
        let status = std::fs::read_to_string(format!("/proc/{}/status", self.child.id())).ok()?;
        let line = status.lines().find(|line| line.starts_with(name))?;
        line[name.len()..].split_whitespace().next()?.parse().ok()
    }

    #[cfg(target_os = "linux")]
    fn rss(&self) -> u64 {
        self.status_field("VmRSS:").unwrap_or(0) * 1024
    }

    #[cfg(target_os = "linux")]
    fn threads(&self) -> Option<u64> {
        self.status_field("Threads:")
    }

    #[cfg(target_os = "linux")]
    fn handles(&self) -> Option<u64> {
        let fds = std::fs::read_dir(format!("/proc/{}/fd", self.child.id())).ok()?;
        Some(fds.count() as u64)
    }

    #[cfg(windows)]
    fn rss(&self) -> u64 {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: the child's process handle stays open for `self`'s lifetime.
        let ok = unsafe { GetProcessMemoryInfo(self.child.as_raw_handle(), &mut counters, size) };
        if ok == 0 { 0 } else { counters.WorkingSetSize as u64 }
    }

    #[cfg(windows)]
    fn threads(&self) -> Option<u64> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
            Thread32Next,
        };
        // SAFETY: the snapshot handle is closed below, and `entry.dwSize` is set as required.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut entry: THREADENTRY32 = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut count = 0;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if entry.th32OwnerProcessID == self.child.id() {
                    count += 1;
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            Some(count)
        }
    }

    #[cfg(windows)]
    fn handles(&self) -> Option<u64> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Threading::GetProcessHandleCount;
        let mut count = 0u32;
        // SAFETY: the child's process handle stays open for `self`'s lifetime.
        let ok = unsafe { GetProcessHandleCount(self.child.as_raw_handle(), &mut count) };
        (ok != 0).then_some(u64::from(count))
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn rss(&self) -> u64 {
        let output = Command::new("ps")
            .args(["-o", "rss=", "-p", &self.child.id().to_string()])
            .output()
            .ok();
        output
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok())
            .unwrap_or(0)
            * 1024
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn threads(&self) -> Option<u64> {
        None
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    fn handles(&self) -> Option<u64> {
        None
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
