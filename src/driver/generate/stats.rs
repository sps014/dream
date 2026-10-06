//! Process-wide generator counters, so tests and `-v` can tell a cache hit from a rebuild.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
struct Counters {
    passes: AtomicU64,
    fast_path_skips: AtomicU64,
    exe_builds: AtomicU64,
    exe_cache_hits: AtomicU64,
    runs: AtomicU64,
    replays: AtomicU64,
}

static COUNTERS: Counters = Counters {
    passes: AtomicU64::new(0),
    fast_path_skips: AtomicU64::new(0),
    exe_builds: AtomicU64::new(0),
    exe_cache_hits: AtomicU64::new(0),
    runs: AtomicU64::new(0),
    replays: AtomicU64::new(0),
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GenStats {
    /// Generator passes entered (one per compile at a stage that runs generators).
    pub passes: u64,
    /// Passes that ended before building a snapshot because no trigger matched.
    pub fast_path_skips: u64,
    pub exe_builds: u64,
    pub exe_cache_hits: u64,
    /// Generator executions.
    pub runs: u64,
    /// `@incremental` results replayed from the cache instead of running.
    pub replays: u64,
}

impl GenStats {
    pub fn current() -> Self {
        let c = &COUNTERS;
        GenStats {
            passes: c.passes.load(Ordering::Relaxed),
            fast_path_skips: c.fast_path_skips.load(Ordering::Relaxed),
            exe_builds: c.exe_builds.load(Ordering::Relaxed),
            exe_cache_hits: c.exe_cache_hits.load(Ordering::Relaxed),
            runs: c.runs.load(Ordering::Relaxed),
            replays: c.replays.load(Ordering::Relaxed),
        }
    }

    /// Counter growth since `earlier`.
    pub fn since(self, earlier: GenStats) -> GenStats {
        GenStats {
            passes: self.passes - earlier.passes,
            fast_path_skips: self.fast_path_skips - earlier.fast_path_skips,
            exe_builds: self.exe_builds - earlier.exe_builds,
            exe_cache_hits: self.exe_cache_hits - earlier.exe_cache_hits,
            runs: self.runs - earlier.runs,
            replays: self.replays - earlier.replays,
        }
    }
}

pub(super) enum Event {
    Pass,
    FastPathSkip,
    #[cfg(feature = "native")]
    ExeBuild,
    #[cfg(feature = "native")]
    ExeCacheHit,
    #[cfg(feature = "native")]
    Run,
    #[cfg(feature = "native")]
    Replay,
}

pub(super) fn record(event: Event) {
    let c = &COUNTERS;
    let counter = match event {
        Event::Pass => &c.passes,
        Event::FastPathSkip => &c.fast_path_skips,
        #[cfg(feature = "native")]
        Event::ExeBuild => &c.exe_builds,
        #[cfg(feature = "native")]
        Event::ExeCacheHit => &c.exe_cache_hits,
        #[cfg(feature = "native")]
        Event::Run => &c.runs,
        #[cfg(feature = "native")]
        Event::Replay => &c.replays,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}
