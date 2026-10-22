//! Asynchronous host operations the guest polls by id: start, wait, read the status-prefixed
//! result, cancel, claim owned resources, and release.

use crate::{ForeignFuture, complete_foreign_future, lock};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::{runtime::Runtime, sync::watch, task::AbortHandle};

/// An error code (`EIO`, `ECLOSED`, ...) and message for the guest.
pub type Failure = (&'static str, String);

/// A resource an operation result owns until the guest claims it.
pub trait Detach: Send + 'static {
    /// Keeps the resource alive after the result is dropped; the guest now owns it.
    fn detach(&mut self);
}

/// A completed operation's bytes plus an optional resource handed to the guest on claim.
pub struct Output<C> {
    pub bytes: Vec<u8>,
    pub child: Option<C>,
}

impl<C> Output<C> {
    pub fn bytes(bytes: Vec<u8>) -> Self {
        Self { bytes, child: None }
    }
}

/// The guest-facing `{"code","message"}` error record.
pub fn error_json(code: &str, message: &str) -> String {
    serde_json::json!({"code": code, "message": message}).to_string()
}

struct Operation<C> {
    result: Option<Result<Output<C>, Failure>>,
    waiters: Vec<ForeignFuture>,
    cancel: watch::Sender<bool>,
    task: Option<AbortHandle>,
}

struct Registry<C> {
    next: i32,
    operations: BTreeMap<i32, Arc<Mutex<Operation<C>>>>,
}

/// One capability's operation table; `runtime` supplies the Tokio runtime work is spawned on.
pub struct Operations<C> {
    registry: OnceLock<Mutex<Registry<C>>>,
    runtime: fn() -> Result<&'static Runtime, Failure>,
}

impl<C: Detach> Operations<C> {
    pub const fn new(runtime: fn() -> Result<&'static Runtime, Failure>) -> Self {
        Self {
            registry: OnceLock::new(),
            runtime,
        }
    }

    fn registry(&self) -> &Mutex<Registry<C>> {
        self.registry.get_or_init(|| {
            Mutex::new(Registry {
                next: 0,
                operations: BTreeMap::new(),
            })
        })
    }

    fn get(&self, id: i32) -> Option<Arc<Mutex<Operation<C>>>> {
        lock(self.registry()).operations.get(&id).cloned()
    }

    /// Spawns `work` and returns its id, or 0 when the table is full. `work` receives a
    /// receiver that flips to `true` on cancellation.
    pub fn start<F, W>(&'static self, work: W) -> i32
    where
        F: Future<Output = Result<Output<C>, Failure>> + Send + 'static,
        W: FnOnce(watch::Receiver<bool>) -> F + Send + 'static,
    {
        let (cancel, cancellation) = watch::channel(false);
        let mut registry = lock(self.registry());
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
        match (self.runtime)() {
            Err(error) => {
                lock(&state).result = Some(Err(error));
            }
            Ok(runtime) => {
                let completion = state.clone();
                let task = runtime.spawn(async move {
                    let result = work(cancellation).await;
                    let waiters = {
                        let mut operation = lock(&completion);
                        operation.result = Some(result);
                        std::mem::take(&mut operation.waiters)
                    };
                    for waiter in waiters {
                        complete_foreign_future(waiter, 0);
                    }
                });
                lock(&state).task = Some(task.abort_handle());
            }
        }
        id
    }

    /// Registers `waiter` for completion; false when the operation is unknown or already done.
    pub fn wait(&self, id: i32, waiter: ForeignFuture) -> bool {
        let Some(operation) = self.get(id) else {
            return false;
        };
        let mut operation = lock(&operation);
        if operation.result.is_some() {
            return false;
        }
        operation.waiters.push(waiter);
        true
    }

    /// Status byte 0 (pending), 1 (bytes follow), or 2 (error JSON follows, empty if unknown).
    pub fn result(&self, id: i32) -> Vec<u8> {
        let Some(operation) = self.get(id) else {
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
                output.extend_from_slice(error_json(code, message).as_bytes());
                output
            }
        }
    }

    /// Drops the operation, aborting unfinished work and waking any waiters.
    pub fn release(&self, id: i32) {
        let operation = lock(self.registry()).operations.remove(&id);
        if let Some(operation) = operation {
            let mut operation = lock(&operation);
            if let Some(task) = operation.task.take() {
                task.abort();
            }
            for waiter in std::mem::take(&mut operation.waiters) {
                complete_foreign_future(waiter, 0);
            }
        }
    }

    pub fn cancel(&self, id: i32) {
        if let Some(operation) = self.get(id) {
            let _ = lock(&operation).cancel.send(true);
        }
    }

    /// Transfers the result's resource to the guest so releasing the operation keeps it open.
    pub fn claim(&self, id: i32) {
        if let Some(operation) = self.get(id) {
            let mut operation = lock(&operation);
            if let Some(Ok(output)) = operation.result.as_mut()
                && let Some(mut child) = output.child.take()
            {
                child.detach();
            }
        }
    }
}
