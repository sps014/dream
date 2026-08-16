//! Async frame ABI constants. Poll bodies are ordinary MIR CFGs (`Await` / `AsyncComplete`);
//! LLVM lowers them. The scheduler lives in `dream-rt`.

pub const F_STATE: i32 = 0;
pub const F_RESULT: i32 = 8;
pub const F_WIDE: i32 = 56;
pub const F_AWAITING: i32 = 20;
pub const F_SLOTS: i32 = 64;
pub const KIND_HOST: i32 = 1;
pub const HOST_POLL_INDEX: i32 = -1;
