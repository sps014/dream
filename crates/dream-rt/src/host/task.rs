//! Offload un-awaited async MIR calls so the owner can `Promise.cancel` / overlap workers.
//!
//! LLVM flattens `Await` into a blocking call. Running the callee on a helper thread returns a
//! ticket immediately; `dream_task_join_if` waits when the owner actually awaits.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, OnceLock};
use std::thread;

static NEXT: AtomicI32 = AtomicI32::new(0x5000_0000);

struct Task {
    rx: Option<Receiver<i32>>,
    workers: Vec<i32>,
}

fn tasks() -> &'static Mutex<HashMap<i32, Task>> {
    static T: OnceLock<Mutex<HashMap<i32, Task>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

thread_local! {
    static CURRENT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

fn start_task(body: impl FnOnce() -> i32 + Send + 'static) -> i32 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    tasks().lock().unwrap().insert(
        id,
        Task {
            rx: Some(rx),
            workers: Vec::new(),
        },
    );
    let _ = thread::Builder::new()
        .name(format!("dream-task-{id}"))
        .spawn(move || {
            CURRENT.with(|c| c.set(id));
            let v = body();
            let _ = tx.send(v);
        });
    id
}

pub(crate) fn on_worker_spawn(worker_id: i32) {
    let tid = CURRENT.with(|c| c.get());
    if tid == 0 {
        return;
    }
    if let Some(t) = tasks().lock().unwrap().get_mut(&tid) {
        t.workers.push(worker_id);
    }
}

fn join_task(id: i32) -> Option<i32> {
    let rx = tasks().lock().unwrap().get_mut(&id)?.rx.take()?;
    rx.recv().ok()
}

#[no_mangle]
pub extern "C" fn dream_task_join_if(p: i32) -> i32 {
    join_task(p).unwrap_or(p)
}

#[no_mangle]
pub extern "C" fn dream_cancel(p: i32) {
    let workers = tasks()
        .lock()
        .unwrap()
        .get(&p)
        .map(|t| t.workers.clone())
        .unwrap_or_default();
    for id in workers {
        super::worker::terminate_worker(id);
    }
}

#[no_mangle]
pub extern "C" fn dream_task_run0(f: extern "C" fn() -> i32) -> i32 {
    start_task(move || f())
}

#[no_mangle]
pub extern "C" fn dream_task_run1(f: extern "C" fn(i32) -> i32, a0: i32) -> i32 {
    start_task(move || f(a0))
}

#[no_mangle]
pub extern "C" fn dream_task_run2(f: extern "C" fn(i32, i32) -> i32, a0: i32, a1: i32) -> i32 {
    start_task(move || f(a0, a1))
}

#[no_mangle]
pub extern "C" fn dream_task_run3(
    f: extern "C" fn(i32, i32, i32) -> i32,
    a0: i32,
    a1: i32,
    a2: i32,
) -> i32 {
    start_task(move || f(a0, a1, a2))
}
