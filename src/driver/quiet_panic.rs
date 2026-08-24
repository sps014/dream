//! Thread-local panic-output suppression for the ICE catch in [`crate::driver::compiler`].
//!
//! `catch_unwind` around code generation converts internal panics into clean
//! `CompileError::Internal` messages, but Rust's default hook would still print the raw
//! "thread '...' panicked at ..." header. The previous implementation swapped the
//! process-global hook for a no-op and serialized the swap with a mutex held across the whole
//! codegen phase — which serialized every parallel `compile()` call (the entire test corpus)
//! through its heaviest section, and briefly swallowed *other threads'* genuine panic output.
//!
//! Instead, one delegating hook is installed process-wide on first use; it prints unless the
//! *panicking thread* has opted into silence via this module's guard. Per-call cost is a
//! thread-local flag flip; concurrent compiles never block each other.

use std::cell::Cell;
use std::sync::OnceLock;

thread_local! {
    static SUPPRESSED: Cell<bool> = const { Cell::new(false) };
}

fn suppressed() -> bool {
    SUPPRESSED.with(Cell::get)
}

fn install_hook() {
    // Runs exactly once per process: capture the real hook and replace it permanently with a
    // delegator. Because the swap happens once — never per compile call — there is no race to
    // serialize, which is what forced the old global-mutex design.
    HOOK_INSTALLED.get_or_init(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !suppressed() {
                prev(info);
            }
        }));
    });
}

static HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

/// RAII guard: suppresses Rust's default panic output for panics raised **on this thread**
/// while the guard is alive. Nesting-safe (restores the prior value).
///
/// Wrap only the narrow region that legitimately catches panics — e.g. an
/// `AssertUnwindSafe` closure around codegen — so genuine panics elsewhere still render.
pub(crate) struct QuietPanics {
    prev: bool,
}

impl QuietPanics {
    pub(crate) fn new() -> Self {
        install_hook();
        QuietPanics {
            prev: SUPPRESSED.with(|c| c.replace(true)),
        }
    }
}

impl Default for QuietPanics {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for QuietPanics {
    fn drop(&mut self) {
        SUPPRESSED.with(|c| c.set(self.prev));
    }
}
