//! Language-defined identifiers the compiler recognizes by spelling: the method receiver and the
//! `Option` / `Result` variants that `?`, `for`, entry points, and marshalers rely on.
//!
//! The spellings live in [`crate::core_items`]; this module re-exports them for existing callers.

pub use crate::core_items::{OPTION_NONE, OPTION_SOME, RESULT_ERR, RESULT_OK, THIS_NAME};
