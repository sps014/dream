//! Entry-point (`main`) return-value rules, shared by signature validation, the
//! all-paths-return check, and HIR emission so the three cannot drift apart.
//!
//! `main`'s return value is the process exit status: `void` exits 0, `int` *is* the exit code,
//! and `Result<T, E>` reports an `Err` on stderr and exits 1.

use dream_syntax::nodes::{FunctionNode, Type};

/// The fixed name the entry point is exported under (`dream_mir::abi::ENTRY_FN`, restated here
/// because `dream-sema` must not depend on the backend).
pub(crate) const ENTRY_NAME: &str = "main";

/// The value `main` returns when control reaches the end of its body without an explicit `return`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryTail {
    /// `main(): int` — falling off the end means success, as in C99.
    Zero,
    /// `main(): Result<bool, E>` — Dream has no unit type, so `bool` is the "no useful value"
    /// payload and reaching the end is `Result.Ok(true)`.
    OkTrue,
}

/// The implicit tail return for `function`, or `None` when it must return on every path.
///
/// Deliberately narrow: only the top-level entry point qualifies, and only for the two spellings
/// whose success value is unambiguous. Methods never match — they are renamed to `{Type}_{method}`
/// before analysis — and any other `Result<T, E>` still needs an explicit `return`.
pub(crate) fn entry_tail_return(function: &FunctionNode<'_>) -> Option<EntryTail> {
    if function.name.text != ENTRY_NAME {
        return None;
    }
    match function.return_type.as_ref()? {
        Type::Integer(_) => Some(EntryTail::Zero),
        Type::Struct(name, Some(args))
            if name.text == "Result" && args.len() == 2 && matches!(args[0], Type::Boolean(_)) =>
        {
            Some(EntryTail::OkTrue)
        }
        _ => None,
    }
}
