//! Opt-in GPU frame timing (`DREAM_GPU_PROFILE=1`). Logs a rolling average every N frames.
//! Also appends to `/tmp/dream-gpu-profile.log`.

use std::cell::RefCell;
use std::time::{Duration, Instant};

thread_local! {
    static PROF: RefCell<FrameProf> = const { RefCell::new(FrameProf::new()) };
}

struct FrameProf {
    frames: u32,
    acquire_ns: u128,
    encode_ns: u128,
    present_ns: u128,
    winit_ns: u128,
    gamepad_ns: u128,
    frame_ns: u128,
    last_w: u32,
    last_h: u32,
}

impl FrameProf {
    const fn new() -> Self {
        Self {
            frames: 0,
            acquire_ns: 0,
            encode_ns: 0,
            present_ns: 0,
            winit_ns: 0,
            gamepad_ns: 0,
            frame_ns: 0,
            last_w: 0,
            last_h: 0,
        }
    }
}

fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("DREAM_GPU_PROFILE")
            .map(|v| v != "0" && v != "false")
            .unwrap_or(false)
    })
}

pub struct Span {
    start: Instant,
}

impl Span {
    pub fn start() -> Option<Self> {
        if enabled() {
            Some(Self {
                start: Instant::now(),
            })
        } else {
            None
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

pub fn note_acquire(d: Duration) {
    if !enabled() {
        return;
    }
    PROF.with(|p| p.borrow_mut().acquire_ns += d.as_nanos());
}

pub fn note_encode(d: Duration) {
    if !enabled() {
        return;
    }
    PROF.with(|p| p.borrow_mut().encode_ns += d.as_nanos());
}

pub fn note_present(d: Duration) {
    if !enabled() {
        return;
    }
    PROF.with(|p| p.borrow_mut().present_ns += d.as_nanos());
}

pub fn note_pump_winit(d: Duration) {
    if !enabled() {
        return;
    }
    PROF.with(|p| p.borrow_mut().winit_ns += d.as_nanos());
}

pub fn note_pump_gamepad(d: Duration) {
    if !enabled() {
        return;
    }
    PROF.with(|p| p.borrow_mut().gamepad_ns += d.as_nanos());
}

pub fn note_size(w: u32, h: u32) {
    if !enabled() {
        return;
    }
    PROF.with(|p| {
        let mut p = p.borrow_mut();
        p.last_w = w;
        p.last_h = h;
    });
}

/// Call once per displayed frame (from `Gpu.frame`).
pub fn end_frame() {
    if !enabled() {
        return;
    }
    PROF.with(|p| {
        p.borrow_mut().frames += 1;
    });
    thread_local! {
        static LAST: RefCell<Option<Instant>> = const { RefCell::new(None) };
    }
    LAST.with(|last| {
        let now = Instant::now();
        let mut slot = last.borrow_mut();
        if let Some(prev) = *slot {
            let dt = prev.elapsed();
            PROF.with(|p| {
                let mut p = p.borrow_mut();
                p.frame_ns += dt.as_nanos();
                if p.frames > 0 && p.frames % 60 == 0 {
                    let n = p.frames as u128;
                    let avg = |sum: u128| (sum / n) as f64 / 1_000_000.0;
                    let line = format!(
                        "Dream GPU profile (avg over {} frames): drawable={}x{}  acquire={:.2}ms  encode={:.2}ms  present={:.2}ms  winit={:.2}ms  gamepad={:.2}ms  frame={:.2}ms ({:.1} FPS)\n",
                        p.frames,
                        p.last_w,
                        p.last_h,
                        avg(p.acquire_ns),
                        avg(p.encode_ns),
                        avg(p.present_ns),
                        avg(p.winit_ns),
                        avg(p.gamepad_ns),
                        avg(p.frame_ns),
                        1000.0 / avg(p.frame_ns).max(0.001),
                    );
                    eprint!("{line}");
                    let _ = std::io::Write::flush(&mut std::io::stderr());
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/dream-gpu-profile.log")
                    {
                        use std::io::Write;
                        let _ = f.write_all(line.as_bytes());
                        let _ = f.flush();
                    }
                    p.acquire_ns = 0;
                    p.encode_ns = 0;
                    p.present_ns = 0;
                    p.winit_ns = 0;
                    p.gamepad_ns = 0;
                    p.frame_ns = 0;
                    p.frames = 0;
                }
            });
        }
        *slot = Some(now);
    });
}
