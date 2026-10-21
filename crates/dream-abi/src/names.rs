//! Language-defined identifiers the compiler recognizes by spelling: the method receiver and the
//! `Option` / `Result` variants that `?`, `for`, entry points, and marshalers rely on.

pub const THIS_NAME: &str = "this";
pub const OPTION_SOME: &str = "Some";
pub const OPTION_NONE: &str = "None";
pub const RESULT_OK: &str = "Ok";
pub const RESULT_ERR: &str = "Err";
