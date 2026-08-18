pub const INDEX_OUT_OF_BOUNDS: &str = "panic: index out of bounds";
pub const DIVIDE_BY_ZERO: &str = "panic: attempt to divide by zero";
pub const INVALID_CAST: &str = "panic: invalid cast";
/// Reading an `unowned` field whose referent has already been deallocated.
pub const UNOWNED_NULL_DEREF: &str = "panic: access to deallocated 'unowned' reference";

/// Every located panic message base, in a fixed order matching [`located_all`].
pub const ALL: [&str; 4] = [
    INDEX_OUT_OF_BOUNDS,
    DIVIDE_BY_ZERO,
    INVALID_CAST,
    UNOWNED_NULL_DEREF,
];

pub fn located(base: &str, file: Option<&str>, func_name: &str, line: u32) -> String {
    let line = if line == 0 {
        "?".to_string()
    } else {
        line.to_string()
    };
    format!(
        "{base} (at {}:{line}, in {func_name})",
        file.unwrap_or("<unknown>")
    )
}

pub fn located_all(file: Option<&str>, func_name: &str, line: u32) -> [String; 4] {
    ALL.map(|base| located(base, file, func_name, line))
}
