//! Native OS threads for `WebWorker`. Same process heap as the owner (`dream-rt` C heap).

use crate::guest;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

extern "C" {
    fn __dream_worker_invoke(idx: i32, env: i32, msg: i32) -> i32;
    fn dream_debug_worker_start(id: u32);
    fn dream_debug_worker_exit(id: u32);
}

enum Job {
    Message(i32, i32, String),
    Terminate,
}

struct WorkerHandle {
    to_worker: Sender<Job>,
    from_worker: Arc<Mutex<Receiver<String>>>,
    reply_tx: Sender<String>,
    fn_idx: i32,
    env: i32,
}

fn workers() -> &'static Mutex<HashMap<u32, WorkerHandle>> {
    static WORKERS: OnceLock<Mutex<HashMap<u32, WorkerHandle>>> = OnceLock::new();
    WORKERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn killed() -> &'static Mutex<HashSet<u32>> {
    static K: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
    K.get_or_init(|| Mutex::new(HashSet::new()))
}

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

fn spawn_worker_thread(fn_idx: i32, env: i32) -> i32 {
    let (job_tx, job_rx) = channel::<Job>();
    let (reply_tx, reply_rx) = channel::<String>();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let reply_for_loop = reply_tx.clone();
    if std::thread::Builder::new()
        .name(format!("dream-worker-{id}"))
        .spawn(move || {
            unsafe { dream_debug_worker_start(id + 1) };
            worker_loop(id, job_rx, reply_for_loop);
            unsafe { dream_debug_worker_exit(id + 1) };
        })
        .is_err()
    {
        eprintln!("dream worker: failed to spawn thread");
        return 0;
    }
    workers().lock().unwrap().insert(
        id,
        WorkerHandle {
            to_worker: job_tx,
            from_worker: Arc::new(Mutex::new(reply_rx)),
            reply_tx,
            fn_idx,
            env,
        },
    );
    super::task::on_worker_spawn(id as i32);
    id as i32
}

fn worker_loop(id: u32, job_rx: Receiver<Job>, reply_tx: Sender<String>) {
    while let Ok(job) = job_rx.recv() {
        if killed().lock().unwrap().contains(&id) {
            break;
        }
        match job {
            Job::Terminate => break,
            Job::Message(fn_idx, env, msg) => {
                let ptr = guest::intern(&msg);
                let reply_ptr = std::panic::catch_unwind(|| unsafe {
                    __dream_worker_invoke(fn_idx, env, ptr)
                })
                .unwrap_or(0);
                let reply = guest::read_string(reply_ptr);
                if reply_tx.send(reply).is_err() {
                    break;
                }
            }
        }
    }
    killed().lock().unwrap().remove(&id);
}

#[no_mangle]
pub extern "C" fn dream_worker_spawn(fn_idx: i32, env: i32) -> i32 {
    spawn_worker_thread(fn_idx, env)
}

#[no_mangle]
pub extern "C" fn dream_worker_pool_spawn() -> i32 {
    spawn_worker_thread(0, 0)
}

#[no_mangle]
pub extern "C" fn dream_worker_post(id: i32, msg: i32) {
    let text = guest::read_string(msg);
    let g = workers().lock().unwrap();
    if let Some(w) = g.get(&(id as u32)) {
        let _ = w.to_worker.send(Job::Message(w.fn_idx, w.env, text));
    }
}

#[no_mangle]
pub extern "C" fn dream_worker_recv(id: i32) -> i32 {
    let rx = {
        let g = workers().lock().unwrap();
        g.get(&(id as u32)).map(|w| w.from_worker.clone())
    };
    let Some(rx) = rx else {
        return guest::intern("");
    };
    let text = rx.lock().unwrap().recv().unwrap_or_default();
    guest::intern(&text)
}

pub(crate) fn terminate_worker(id: i32) {
    killed().lock().unwrap().insert(id as u32);
    let g = workers().lock().unwrap();
    if let Some(w) = g.get(&(id as u32)) {
        let _ = w.reply_tx.send(String::new());
        let _ = w.to_worker.send(Job::Terminate);
    }
}

#[no_mangle]
pub extern "C" fn dream_worker_terminate(id: i32) {
    terminate_worker(id);
}

#[no_mangle]
pub extern "C" fn dream_worker_pool_dispatch(id: i32, fn_idx: i32, env: i32, msg: i32) -> i32 {
    let text = guest::read_string(msg);
    let rx = {
        let g = workers().lock().unwrap();
        if let Some(w) = g.get(&(id as u32)) {
            let _ = w.to_worker.send(Job::Message(fn_idx, env, text));
            Some(w.from_worker.clone())
        } else {
            None
        }
    };
    let Some(rx) = rx else {
        return guest::intern("");
    };
    let reply = rx.lock().unwrap().recv().unwrap_or_default();
    guest::intern(&reply)
}
