use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

pub struct HttpFixture {
    pub url: String,
    pub accepted: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl HttpFixture {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let accepted = Arc::new(AtomicUsize::new(0));
        let (stopping, connections) = (stop.clone(), accepted.clone());
        let worker = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        connections.fetch_add(1, Ordering::Relaxed);
                        thread::spawn(move || serve(stream));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(1))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url: format!("http://{address}"),
            accepted,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn serve(mut stream: TcpStream) {
    // macOS inherits listener flags; connection workers require blocking reads.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    loop {
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read_exact(&mut byte).is_err() {
                return;
            }
            headers.push(byte[0]);
            if headers.len() > 65536 {
                return;
            }
        }
        let text = String::from_utf8_lossy(&headers);
        let mut first = text.lines().next().unwrap_or("").split_whitespace();
        let method = first.next().unwrap_or("");
        let path = first.next().unwrap_or("");
        let length = text
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        let mut body = vec![0; length.min(1048576)];
        if stream.read_exact(&mut body).is_err() {
            return;
        }
        if path == "/disconnect" {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort");
            return;
        }
        if path == "/slow" {
            thread::sleep(Duration::from_millis(150));
        }
        if path == "/stream" || path == "/slow-body" {
            if stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n5\r\nfirst\r\n").is_err() { return; }
            thread::sleep(Duration::from_millis(if path == "/slow-body" {
                150
            } else {
                10
            }));
            if stream.write_all(b"6\r\nsecond\r\n0\r\n\r\n").is_err() {
                return;
            }
            continue;
        }
        let (status, extra, payload) = match path {
            "/cookies/set" => (
                200,
                "Set-Cookie: session=stored; Path=/cookies\r\n".to_string(),
                Vec::new(),
            ),
            "/cookies/delete" => (
                200,
                "Set-Cookie: session=; Path=/cookies; Max-Age=0\r\n".to_string(),
                Vec::new(),
            ),
            "/cookies/redirect" => (
                302,
                "Set-Cookie: redirect=stored; Path=/cookies\r\nLocation: /cookies/echo\r\n"
                    .to_string(),
                Vec::new(),
            ),
            "/cookies/cross" => (
                302,
                format!(
                    "Location: http://localhost:{}/cookies/echo\r\n",
                    stream.local_addr().unwrap().port()
                ),
                Vec::new(),
            ),
            "/cookies/echo" => (
                200,
                String::new(),
                text.lines()
                    .filter_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("cookie").then(|| value.trim())
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
                    .into_bytes(),
            ),
            "/redirect" => (302, "Location: /stream\r\n".to_string(), Vec::new()),
            "/loop" => (302, "Location: /loop\r\n".to_string(), Vec::new()),
            "/echo" => (404, format!("X-Method: {method}\r\n"), body),
            "/invalid-utf8" => (200, String::new(), vec![255]),
            _ => (200, String::new(), b"hello".to_vec()),
        };
        let header = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: keep-alive\r\n{extra}\r\n",
            payload.len()
        );
        if stream.write_all(header.as_bytes()).is_err()
            || method != "HEAD" && stream.write_all(&payload).is_err()
        {
            return;
        }
    }
}
