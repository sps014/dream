//! Compile-time ownership tokens for RC locals.
//!
//! Each owned reference local holds at most one token (the +1 count). Tokens move on last-use
//! assign/sink, stay put on `borrow`, and die at last-use destroy, join balancing, or return.
//! This is CFG dataflow, not ownership-SSA.
mod aliases;
mod analysis;
mod calls;
mod destroy;
mod flow;
mod locals;
mod statements;

pub(crate) use aliases::{
    funcbox_env_rc_roots, leftover_alias_parent, leftover_keep, leftover_order,
};
pub(crate) use analysis::TokenAnalysis;
pub(crate) use calls::{call_escape_locals, move_source, sink_call_args, take_arg_effects};
pub(crate) use locals::{
    assigns_local, dest_holds_token, is_owned_local, needs_rebind_temp, null_local, rc_op_on_local,
    release_and_null, source_line_end, terminator_reads_local,
};
pub(crate) use statements::apply_stmt_tokens;
