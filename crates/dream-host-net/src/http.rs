use reqwest::{
    Client, Response,
    header::{HeaderName, HeaderValue},
};
use serde::Deserialize;
use dream_host_abi::handles::HandleTable;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex as AsyncMutex, watch},
    task::AbortHandle,
};

use crate::socket_operations::{Failure, runtime};
type ClientPools = BTreeMap<(u64, i32), Client>;
struct ClientState {
    pools: Mutex<ClientPools>,
    closed: watch::Sender<bool>,
}
struct BodyState {
    response: Response,
    remainder: Vec<u8>,
    offset: usize,
    eof: bool,
}
struct Body {
    state: AsyncMutex<BodyState>,
    busy: AtomicBool,
    closed: watch::Sender<bool>,
}
enum Output {
    Response(Response),
    Bytes(Vec<u8>),
}
struct Operation {
    result: Option<Result<Output, Failure>>,
    task: Option<AbortHandle>,
    response_body: Option<i32>,
    waiter: Option<dream_host_abi::ForeignFuture>,
}
struct Registry {
    clients: HandleTable<Arc<ClientState>>,
    bodies: HandleTable<Arc<Body>>,
    operations: HandleTable<Arc<Mutex<Operation>>>,
}
use dream_host_abi::lock;
fn registry() -> &'static Mutex<Registry> {
    static STATE: Mutex<Registry> = Mutex::new(Registry {
        clients: HandleTable::new(),
        bodies: HandleTable::new(),
        operations: HandleTable::new(),
    });
    &STATE
}
fn failure(code: &'static str, text: impl ToString) -> Failure {
    (code, text.to_string())
}
fn transport(e: reqwest::Error) -> Failure {
    let code = if e.is_timeout() {
        "ETIMEOUT"
    } else if e.is_redirect() {
        "EREDIRECT"
    } else if e.is_builder() {
        "EINVAL"
    } else {
        "ETRANSPORT"
    };
    // reqwest errors may contain credentials or request query strings.
    failure(code, e.without_url())
}
#[derive(Deserialize)]
struct Request {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    timeout_ms: u64,
    connect_timeout_ms: u64,
    max_redirects: i32,
}
impl ClientState {
    fn client(&self, request: &Request) -> Result<Client, Failure> {
        let key = (request.connect_timeout_ms, request.max_redirects);
        let mut pools = lock(&self.pools);
        if let Some(client) = pools.get(&key) {
            return Ok(client.clone());
        }
        if pools.len() >= 32 {
            return Err(failure("ELIMIT", "too many client option combinations"));
        }
        let _guard = runtime()?.enter();
        let mut builder = Client::builder().redirect(if request.max_redirects == 0 {
            reqwest::redirect::Policy::none()
        } else {
            reqwest::redirect::Policy::limited(if request.max_redirects < 0 {
                10
            } else {
                request.max_redirects as usize
            })
        });
        if request.connect_timeout_ms > 0 {
            builder = builder.connect_timeout(Duration::from_millis(request.connect_timeout_ms));
        }
        let client = builder.build().map_err(transport)?;
        pools.insert(key, client.clone());
        Ok(client)
    }
}
pub(crate) fn client_create() -> i32 {
    lock(registry())
        .clients
        .insert(Arc::new(ClientState {
            pools: Mutex::default(),
            closed: watch::channel(false).0,
        }))
        .unwrap_or(0)
}
pub(crate) fn client_close(id: i32) {
    if let Some(client) = lock(registry()).clients.remove(id) {
        client.closed.send_replace(true);
    }
}
fn start(future: impl Future<Output = Result<Output, Failure>> + Send + 'static) -> i32 {
    let Ok(rt) = runtime() else {
        return 0;
    };
    let operation = Arc::new(Mutex::new(Operation {
        result: None,
        task: None,
        response_body: None,
        waiter: None,
    }));
    let mut state = lock(registry());
    let Some(id) = state.operations.insert(operation.clone()) else {
        return 0;
    };
    let task_operation = operation.clone();
    let task = rt.spawn(async move {
        let result = future.await;
        let waiter = {
            let mut operation = lock(&task_operation);
            operation.result = Some(result);
            operation.waiter.take()
        };
        if let Some(waiter) = waiter {
            dream_host_abi::complete_foreign_future(waiter, 0);
        }
    });
    lock(&operation).task = Some(task.abort_handle());
    id
}
pub(crate) fn send(client: i32, json: &str, bytes: Vec<u8>) -> i32 {
    let request = serde_json::from_str::<Request>(json).map_err(|e| failure("EINVAL", e));
    let client = lock(registry()).clients.get(client).cloned();
    let prepared = (|| {
        let request = request?;
        if request.max_redirects < -1 {
            return Err(failure("EINVAL", "invalid redirect limit"));
        }
        let client = client.ok_or_else(|| failure("ECLOSED", "HTTP client is closed"))?;
        let url = url::Url::parse(&request.url).map_err(|e| failure("EINVAL", e))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(failure(
                "EINVAL",
                "HTTP URL must use http/https without credentials",
            ));
        }
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|e| failure("EINVAL", e))?;
        if matches!(method, reqwest::Method::GET | reqwest::Method::HEAD) && !bytes.is_empty() {
            return Err(failure("EINVAL", "GET and HEAD bodies are unsupported"));
        }
        if !matches!(
            request.method.as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
        ) {
            return Err(failure("EINVAL", "unsupported HTTP method"));
        }
        let mut builder = client.client(&request)?.request(method, url).body(bytes);
        for (name, value) in request.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|e| failure("EINVAL", e))?;
            let value = HeaderValue::from_str(&value).map_err(|e| failure("EINVAL", e))?;
            builder = builder.header(name, value);
        }
        if request.timeout_ms > 0 {
            builder = builder.timeout(Duration::from_millis(request.timeout_ms));
        }
        Ok((builder, client))
    })();
    start(async move {
        let (builder, client) = prepared?;
        let mut closed = client.closed.subscribe();
        if *closed.borrow() {
            return Err(failure("ECLOSED", "HTTP client is closed"));
        }
        tokio::select! {
            response = builder.send() => response.map(Output::Response).map_err(transport),
            _ = closed.changed() => Err(failure("ECLOSED", "HTTP client was closed")),
        }
    })
}
struct Busy(Arc<Body>);
impl Drop for Busy {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::Release);
    }
}
pub(crate) fn read(id: i32, count: i32) -> i32 {
    let body = lock(registry()).bodies.get(id).cloned();
    let prepared = (|| {
        if count < 0 {
            return Err(failure("EINVAL", "negative read size"));
        }
        let body = body.ok_or_else(|| failure("ECLOSED", "response body is closed"))?;
        if body.busy.swap(true, Ordering::AcqRel) {
            return Err(failure("EBUSY", "response body is busy"));
        }
        Ok(Busy(body))
    })();
    start(async move {
        let busy = prepared?;
        let mut closed = busy.0.closed.subscribe();
        if *closed.borrow() {
            return Err(failure("ECLOSED", "response body is closed"));
        }
        let mut state = busy.0.state.lock().await;
        if count == 0 {
            return Ok(Output::Bytes(Vec::new()));
        }
        while state.offset == state.remainder.len() && !state.eof {
            let chunk = tokio::select! {
                chunk = state.response.chunk() => chunk.map_err(transport)?,
                _ = closed.changed() => return Err(failure("ECLOSED", "response body was closed")),
            };
            state.eof = chunk.is_none();
            state.remainder = chunk.map(|b| b.to_vec()).unwrap_or_default();
            state.offset = 0;
        }
        let end = state.remainder.len().min(state.offset + count as usize);
        let bytes = state.remainder[state.offset..end].to_vec();
        state.offset = end;
        Ok(Output::Bytes(bytes))
    })
}
pub(crate) fn body_close(id: i32) {
    if let Some(body) = lock(registry()).bodies.remove(id) {
        body.closed.send_replace(true);
    }
}
pub(crate) fn claim(id: i32) {
    if let Some(operation) = lock(registry()).operations.get(id) {
        lock(operation).response_body = None;
    }
}
pub(crate) fn release(id: i32) {
    let operation = lock(registry()).operations.remove(id);
    if let Some(operation) = operation {
        let (body, waiter) = {
            let mut operation = lock(&operation);
            if let Some(task) = &operation.task {
                task.abort();
            }
            (operation.response_body.take(), operation.waiter.take())
        };
        if let Some(waiter) = waiter {
            dream_host_abi::complete_foreign_future(waiter, 0);
        }
        // Poll/claim acquire registry before operation; never reacquire registry while holding it.
        if let Some(body) = body {
            body_close(body);
        }
    }
}
pub(crate) fn wait(id: i32, waiter: dream_host_abi::ForeignFuture) -> bool {
    let operation = lock(registry()).operations.get(id).cloned();
    let Some(operation) = operation else {
        return false;
    };
    let mut operation = lock(&operation);
    if operation.result.is_some() || operation.waiter.is_some() {
        return false;
    }
    operation.waiter = Some(waiter);
    true
}

pub(crate) fn poll(id: i32) -> Vec<u8> {
    let mut state = lock(registry());
    let Some(operation) = state.operations.get(id).cloned() else {
        return error_bytes(failure("ECLOSED", "HTTP operation is closed"));
    };
    let Some(result) = lock(&operation).result.take() else {
        return vec![0];
    };
    match result {
        Err(e) => error_bytes(e),
        Ok(Output::Bytes(bytes)) => {
            let mut wire = vec![1];
            wire.extend(bytes);
            wire
        }
        Ok(Output::Response(response)) => {
            let headers: Vec<_> = response
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_string(),
                        String::from_utf8_lossy(v.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let status = response.status().as_u16();
            let url = response.url().to_string();
            let body = Arc::new(Body {
                state: AsyncMutex::new(BodyState {
                    response,
                    remainder: Vec::new(),
                    offset: 0,
                    eof: false,
                }),
                busy: AtomicBool::new(false),
                closed: watch::channel(false).0,
            });
            let Some(id) = state.bodies.insert(body) else {
                return error_bytes(failure("ELIMIT", "too many open response bodies"));
            };
            let metadata = serde_json::json!({"body": id, "status": status, "url": url, "headers": headers});
            lock(&operation).response_body = Some(id);
            let mut wire = vec![1];
            wire.extend(metadata.to_string().as_bytes());
            wire
        }
    }
}
fn error_bytes(error: Failure) -> Vec<u8> {
    let mut bytes = vec![2];
    bytes.extend(
        serde_json::json!({"code":error.0,"message":error.1})
            .to_string()
            .as_bytes(),
    );
    bytes
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
