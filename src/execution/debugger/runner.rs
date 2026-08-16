//! Runs the compiled native guest under DAP: `dlopen` the debug shared library, install
//! `dream_debug.*` hooks, and call `dream_user_main` on a helper thread.

use serde_json::json;
use std::os::raw::c_char;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

use super::decode::snapshot_locals;
use super::sourcemap::SourceMap;
use super::state::{FrameState, Inner, Shared, ThreadHot, ThreadState};
use super::{thread_name, Writer, MAIN_THREAD};

struct GuestFns {
    heap_base: unsafe extern "C" fn() -> *mut u8,
    heap_cap: unsafe extern "C" fn() -> i32,
    user_main: unsafe extern "C" fn(),
    thread_id: unsafe extern "C" fn() -> i32,
    dbg: Vec<*mut i64>,
}

unsafe impl Send for GuestFns {}
unsafe impl Sync for GuestFns {}

struct Session {
    shared: Arc<Shared>,
    source_map: Arc<SourceMap>,
    writer: Writer,
    fns: GuestFns,
}

static SESSION: OnceLock<Mutex<Option<Arc<Session>>>> = OnceLock::new();

fn session_cell() -> &'static Mutex<Option<Arc<Session>>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

pub(super) fn spawn_execution(
    artifact: String,
    shared: Arc<Shared>,
    source_map: Arc<SourceMap>,
    writer: Writer,
    stop_on_entry: bool,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        {
            let mut inner = shared.inner.lock().unwrap();
            let t = inner
                .threads
                .entry(MAIN_THREAD)
                .or_insert_with(|| ThreadState::new("main"));
            if stop_on_entry {
                t.mode = super::state::RunMode::StepIn;
            }
        }
        if stop_on_entry {
            shared
                .hot_for(MAIN_THREAD)
                .set_mode(super::state::RunMode::StepIn);
        }
        let _ = writer.lock().unwrap().event(
            "thread",
            json!({ "reason": "started", "threadId": MAIN_THREAD }),
        );

        let result = run_program(&artifact, &shared, &source_map, &writer);
        shared.inner.lock().unwrap().terminated = true;
        let mut w = writer.lock().unwrap();
        if let Err(e) = &result {
            let _ = w.event(
                "output",
                json!({ "category": "stderr", "output": format!("Program terminated: {}\n", e) }),
            );
        }
        let _ = w.event("exited", json!({ "exitCode": 0 }));
        let _ = w.event("terminated", json!({}));
        *session_cell().lock().unwrap() = None;
    })
}

fn guest_path(artifact: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(artifact);
    let ext = dream_llvm::shared_lib_ext();
    let lib = p.with_extension(ext);
    if lib.exists() {
        lib
    } else {
        p.with_extension("out")
    }
}

fn run_program(
    artifact: &str,
    shared: &Arc<Shared>,
    source_map: &Arc<SourceMap>,
    writer: &Writer,
) -> Result<(), String> {
    let path = guest_path(artifact);
    let lib = unsafe { libloading::Library::new(&path) }
        .map_err(|e| format!("dlopen {}: {e}", path.display()))?;

    type Install = unsafe extern "C" fn(
        Option<unsafe extern "C" fn(i32)>,
        Option<unsafe extern "C" fn(i32)>,
        Option<unsafe extern "C" fn(i32, i32)>,
    );
    type InstallWorker = unsafe extern "C" fn(
        Option<unsafe extern "C" fn(u32)>,
        Option<unsafe extern "C" fn(u32)>,
    );
    type SetPrint = unsafe extern "C" fn(Option<unsafe extern "C" fn(*const c_char, i32)>);
    type SetThread = unsafe extern "C" fn(i32);

    let install: libloading::Symbol<Install> = unsafe {
        lib.get(b"dream_debug_install\0")
            .map_err(|e| format!("dream_debug_install: {e}"))?
    };
    let install_worker: libloading::Symbol<InstallWorker> = unsafe {
        lib.get(b"dream_debug_install_worker\0")
            .map_err(|e| format!("dream_debug_install_worker: {e}"))?
    };
    let set_print: libloading::Symbol<SetPrint> = unsafe {
        lib.get(b"dream_debug_set_print\0")
            .map_err(|e| format!("dream_debug_set_print: {e}"))?
    };
    let set_thread: libloading::Symbol<SetThread> = unsafe {
        lib.get(b"dream_debug_set_thread\0")
            .map_err(|e| format!("dream_debug_set_thread: {e}"))?
    };
    let rt_init: libloading::Symbol<unsafe extern "C" fn()> = unsafe {
        lib.get(b"dream_rt_init\0")
            .map_err(|e| format!("dream_rt_init: {e}"))?
    };
    let heap_base: libloading::Symbol<unsafe extern "C" fn() -> *mut u8> =
        unsafe { lib.get(b"dream_heap_base\0").map_err(|e| format!("{e}"))? };
    let heap_cap: libloading::Symbol<unsafe extern "C" fn() -> i32> =
        unsafe { lib.get(b"dream_heap_cap\0").map_err(|e| format!("{e}"))? };
    let user_main: libloading::Symbol<unsafe extern "C" fn()> = unsafe {
        lib.get(b"dream_user_main\0")
            .map_err(|e| format!("dream_user_main: {e}"))?
    };
    let thread_id: libloading::Symbol<unsafe extern "C" fn() -> i32> = unsafe {
        lib.get(b"dream_debug_thread\0")
            .map_err(|e| format!("dream_debug_thread: {e}"))?
    };

    let mut dbg = Vec::new();
    for i in 0..256u32 {
        let name = format!("__dbg_v{i}\0");
        let Ok(sym) = (unsafe { lib.get::<*mut i64>(name.as_bytes()) }) else {
            break;
        };
        dbg.push(*sym);
    }
    if dbg.is_empty() {
        return Err("guest has no __dbg_v* spill globals".into());
    }

    let fns = GuestFns {
        heap_base: *heap_base,
        heap_cap: *heap_cap,
        user_main: *user_main,
        thread_id: *thread_id,
        dbg,
    };
    let sess = Arc::new(Session {
        shared: shared.clone(),
        source_map: source_map.clone(),
        writer: writer.clone(),
        fns,
    });
    let user_main_fn = sess.fns.user_main;
    *session_cell().lock().unwrap() = Some(Arc::clone(&sess));

    unsafe {
        rt_init();
        set_print(Some(on_print));
        set_thread(MAIN_THREAD as i32);
        install(Some(on_enter), Some(on_exit), Some(on_line));
        install_worker(Some(on_worker_start), Some(on_worker_exit));
        user_main_fn();
    }
    *session_cell().lock().unwrap() = None;
    drop(lib);
    Ok(())
}

fn with_session<R>(f: impl FnOnce(&Session) -> R) -> Option<R> {
    let g = session_cell().lock().unwrap();
    g.as_ref().map(|s| f(s))
}

extern "C" fn on_print(s: *const c_char, n: i32) {
    if s.is_null() || n <= 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(s as *const u8, n as usize) };
    let text = String::from_utf8_lossy(bytes).into_owned();
    with_session(|sess| {
        let _ = sess.writer.lock().unwrap().event(
            "output",
            json!({ "category": "stdout", "output": text }),
        );
    });
}

extern "C" fn on_enter(id: i32) {
    let Some(tid) = with_session(|s| unsafe { (s.fns.thread_id)() as u32 }) else {
        return;
    };
    let tid = if tid == 0 { MAIN_THREAD } else { tid };
    with_session(|sess| {
        let hot = sess.shared.hot_for(tid);
        hot.depth.fetch_add(1, Ordering::Relaxed);
        let mut inner = sess.shared.inner.lock().unwrap();
        let t = inner
            .threads
            .entry(tid)
            .or_insert_with(|| ThreadState::new(thread_name(tid)));
        if let Some(caller_frame) = t.call_stack.last_mut() {
            let packed = hot.pos.load(Ordering::Relaxed);
            caller_frame.file = (packed >> 32) as u32;
            caller_frame.line = packed as u32;
        }
        t.call_stack.push(FrameState {
            func_id: id as u32,
            file: 0,
            line: 0,
        });
    });
}

extern "C" fn on_exit(_id: i32) {
    let Some(tid) = with_session(|s| unsafe { (s.fns.thread_id)() as u32 }) else {
        return;
    };
    let tid = if tid == 0 { MAIN_THREAD } else { tid };
    with_session(|sess| {
        let hot = sess.shared.hot_for(tid);
        hot.depth.fetch_sub(1, Ordering::Relaxed);
        let mut inner = sess.shared.inner.lock().unwrap();
        if let Some(t) = inner.threads.get_mut(&tid) {
            t.call_stack.pop();
        }
    });
}

extern "C" fn on_line(file_id: i32, line: i32) {
    let Some(tid) = with_session(|s| unsafe { (s.fns.thread_id)() as u32 }) else {
        return;
    };
    let tid = if tid == 0 { MAIN_THREAD } else { tid };
    let Some(sess) = session_cell().lock().unwrap().clone() else {
        return;
    };
    let hot = sess.shared.hot_for(tid);
    hot.pos.store(
        ThreadHot::pack_pos(file_id as u32, line as u32),
        Ordering::Relaxed,
    );
    let maybe_stop = hot.pause.load(Ordering::Relaxed)
        || hot.step_wants_stop()
        || sess.shared.bp_filter.probe(file_id as u32, line as u32);
    if !maybe_stop {
        return;
    }

    let stop = {
        let mut inner = sess.shared.inner.lock().unwrap();
        let Inner {
            breakpoints,
            threads,
            ..
        } = &mut *inner;
        let Some(t) = threads.get_mut(&tid) else {
            return;
        };
        if let Some(frame) = t.call_stack.last_mut() {
            frame.file = file_id as u32;
            frame.line = line as u32;
        }
        Shared::should_stop(breakpoints, t)
    };
    let Some(reason) = stop else {
        return;
    };
    hot.pause.store(false, Ordering::Relaxed);

    let slots: Vec<i64> = sess
        .fns
        .dbg
        .iter()
        .map(|p| unsafe { p.read() })
        .collect();
    let cap = unsafe { (sess.fns.heap_cap)() }.max(0) as usize;
    let heap = unsafe { std::slice::from_raw_parts((sess.fns.heap_base)(), cap) };
    let var_refs = snapshot_locals(heap, &slots, &sess.shared, &sess.source_map, tid);
    {
        let mut inner = sess.shared.inner.lock().unwrap();
        if let Some(t) = inner.threads.get_mut(&tid) {
            t.var_refs = var_refs;
            t.paused = true;
            t.resume = false;
        }
    }
    let _ = sess.writer.lock().unwrap().event(
        "stopped",
        json!({
            "reason": reason.as_str(),
            "threadId": tid,
            "allThreadsStopped": false,
        }),
    );
    let mut inner = sess.shared.inner.lock().unwrap();
    loop {
        match inner.threads.get(&tid) {
            Some(t) if t.resume => break,
            None => return,
            _ => {}
        }
        inner = sess.shared.cv.wait(inner).unwrap();
    }
    if let Some(t) = inner.threads.get_mut(&tid) {
        t.paused = false;
    }
}

extern "C" fn on_worker_start(thread_id: u32) {
    with_session(|sess| {
        {
            let mut inner = sess.shared.inner.lock().unwrap();
            inner
                .threads
                .entry(thread_id)
                .or_insert_with(|| ThreadState::new(thread_name(thread_id)));
        }
        let _ = sess.writer.lock().unwrap().event(
            "thread",
            json!({ "reason": "started", "threadId": thread_id }),
        );
    });
}

extern "C" fn on_worker_exit(thread_id: u32) {
    with_session(|sess| {
        {
            let mut inner = sess.shared.inner.lock().unwrap();
            inner.threads.remove(&thread_id);
        }
        sess.shared.cv.notify_all();
        let _ = sess.writer.lock().unwrap().event(
            "thread",
            json!({ "reason": "exited", "threadId": thread_id }),
        );
    });
}
