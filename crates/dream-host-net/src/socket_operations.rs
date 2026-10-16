use dream_host_abi::ForeignFuture;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};
use tokio::{runtime::Runtime, sync::watch, task::AbortHandle};

pub(crate) type Failure = (&'static str, String);
pub(crate) struct Output {
    pub bytes: Vec<u8>,
    pub child: Option<OwnedResource>,
}
impl Output {
    pub fn bytes(bytes: Vec<u8>) -> Self {
        Self { bytes, child: None }
    }
}
pub(crate) struct OwnedResource {
    id: i32,
    close: fn(i32),
    detached: bool,
}
impl OwnedResource {
    pub fn new(id: i32, close: fn(i32)) -> Self {
        Self {
            id,
            close,
            detached: false,
        }
    }
    pub fn detach(&mut self) {
        self.detached = true;
    }
}
impl Drop for OwnedResource {
    fn drop(&mut self) {
        if !self.detached {
            (self.close)(self.id);
        }
    }
}
struct Operation {
    result: Option<Result<Output, Failure>>,
    waiters: Vec<ForeignFuture>,
    cancel: watch::Sender<bool>,
    task: Option<AbortHandle>,
}
#[derive(Default)]
struct Registry {
    next: i32,
    operations: BTreeMap<i32, Arc<Mutex<Operation>>>,
}
fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|e| e.into_inner())
}
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Mutex::default)
}
pub(crate) fn runtime() -> Result<&'static Runtime, Failure> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            // Transitive TLS clients enable both providers; rustls cannot infer a default.
            let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ("EIO", e.clone()))
}
pub(crate) fn start<F, W>(work: W) -> i32
where
    F: Future<Output = Result<Output, Failure>> + Send + 'static,
    W: FnOnce(watch::Receiver<bool>) -> F + Send + 'static,
{
    let (cancel, cancellation) = watch::channel(false);
    let mut registry = lock(registry());
    if registry.next == i32::MAX || registry.operations.len() >= 4096 {
        return 0;
    }
    registry.next += 1;
    let id = registry.next;
    let state = Arc::new(Mutex::new(Operation {
        result: None,
        waiters: Vec::new(),
        task: None,
        cancel,
    }));
    registry.operations.insert(id, state.clone());
    drop(registry);
    match runtime() {
        Err(error) => {
            lock(&state).result = Some(Err(error));
        }
        Ok(runtime) => {
            let completion = state.clone();
            let task = runtime.spawn(async move {
                let result = work(cancellation).await;
                let waiter = {
                    let mut operation = lock(&completion);
                    operation.result = Some(result);
                    std::mem::take(&mut operation.waiters)
                };
                for waiter in waiter {
                    dream_host_abi::complete_foreign_future(waiter, 0);
                }
            });
            lock(&state).task = Some(task.abort_handle());
        }
    }
    id
}
pub(crate) fn wait(id: i32, waiter: ForeignFuture) -> bool {
    let operation = lock(registry()).operations.get(&id).cloned();
    let Some(operation) = operation else {
        return false;
    };
    let mut operation = lock(&operation);
    if operation.result.is_some() {
        return false;
    }
    operation.waiters.push(waiter);
    true
}
pub(crate) fn result(id: i32) -> Vec<u8> {
    let operation = lock(registry()).operations.get(&id).cloned();
    let Some(operation) = operation else {
        return vec![2];
    };
    let operation = lock(&operation);
    match &operation.result {
        None => vec![0],
        Some(Ok(value)) => {
            let mut output = vec![1];
            output.extend_from_slice(&value.bytes);
            output
        }
        Some(Err((code, message))) => {
            let mut output = vec![2];
            output.extend_from_slice(
                serde_json::json!({"code": code, "message": message})
                    .to_string()
                    .as_bytes(),
            );
            output
        }
    }
}
pub(crate) fn release(id: i32) {
    let operation = lock(registry()).operations.remove(&id);
    if let Some(operation) = operation {
        let mut operation = lock(&operation);
        if let Some(task) = operation.task.take() {
            task.abort();
        }
        for waiter in std::mem::take(&mut operation.waiters) {
            dream_host_abi::complete_foreign_future(waiter, 0);
        }
    }
}

pub(crate) fn cancel(id: i32) {
    if let Some(operation) = lock(registry()).operations.get(&id) {
        let _ = lock(operation).cancel.send(true);
    }
}
pub(crate) fn claim(id: i32) {
    let operation = lock(registry()).operations.get(&id).cloned();
    if let Some(operation) = operation {
        let mut operation = lock(&operation);
        if let Some(Ok(output)) = operation.result.as_mut() {
            if let Some(child) = output.child.as_mut() {
                child.detach();
            }
            output.child.take();
        }
    }
}
