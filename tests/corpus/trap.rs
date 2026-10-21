use super::Case;

pub(super) const CASES: &[Case] = &[
    Case::trap("raw_ptr_size_overflow", "raw memory: size overflow", true),
    Case::trap(
        "raw_ptr_alignment_trap",
        "raw memory: alignment must be a nonzero power of two",
        true,
    ),
    Case::trap(
        "abc_versioned_overrun_trap",
        "6\npanic: index out of bounds\nabc_versioned_overrun_trap.dream:8",
        true,
    ),
    Case::trap(
        "arc_acyclic_resurrection",
        "resurrection of a dying object\nexit code 1",
        true,
    ),
    Case::trap(
        "arc_finalizer_publication",
        "panic: publication of a dying object\nexit code 1",
        true,
    ),
    Case::trap(
        "arc_strong_cycle_leak",
        "intentional strong-cycle leak\nexit code 1",
        true,
    ),
    Case::trap(
        "list_index_oob_trap",
        "3\nList index out of range\nlist_index_oob_trap.dream:11",
        true,
    ),
    Case::trap(
        "lock_unheld_release",
        "panic: lock release requires the owning thread",
        true,
    ),
    Case::trap(
        "lock_wrong_thread_release",
        "panic: lock release requires the owning thread",
        true,
    ),
    Case::trap("main_int_exit", "checking\nexit code 3", true),
    Case::trap(
        "main_result_err",
        "port 8080\nError: ParseError\nexit code 1",
        true,
    ),
    Case::trap(
        "overflow_add_int",
        "start\npanic: attempt to add with overflow\noverflow_add_int.dream:3",
        true,
    ),
    Case::trap(
        "overflow_div_int_min",
        "start\npanic: attempt to divide with overflow",
        true,
    ),
    Case::trap(
        "overflow_neg_int",
        "start\npanic: attempt to negate with overflow",
        true,
    ),
    Case::trap(
        "overflow_shl_int",
        "start\npanic: attempt to shift left with overflow",
        true,
    ),
    Case::trap(
        "panic_array_allocation",
        "before allocation\npanic: array size exceeds the supported limit",
        true,
    ),
    Case::trap(
        "panic_builtin",
        "before\nboom: something went wrong\npanic_builtin.dream:6",
        true,
    ),
    Case::trap(
        "panic_caller_lambda",
        "List index out of range\npanic_caller_lambda.dream:11",
        true,
    ),
    Case::trap(
        "panic_caller_option_struct",
        "1\nunwrap() called on Option.None\npanic_caller_option_struct.dream:25",
        true,
    ),
    Case::trap(
        "panic_caller_package",
        "7\nList index out of range\npanic_caller_package.dream:11",
        true,
    ),
    Case::trap(
        "panic_div_zero",
        "before\npanic: attempt to divide by zero",
        true,
    ),
    Case::trap(
        "panic_index_oob",
        "before\npanic: index out of bounds",
        true,
    ),
    Case::trap("panic_invalid_cast", "before\npanic: invalid cast", true),
    Case::trap(
        "span_range_trap",
        "2\nSpan range out of bounds\nspan_range_trap.dream:7",
        true,
    ),
    Case::trap(
        "string_byte_at_negative_trap",
        "55\npanic: index out of bounds\nstring_byte_at_negative_trap.dream:9",
        true,
    ),
    Case::trap(
        "string_char_at_oob_trap",
        "1\npanic: index out of bounds\nstring_char_at_oob_trap.dream:9",
        true,
    ),
    Case::trap(
        "unowned_field_runtime",
        "before\npanic: 'unowned' reference used after its target was destroyed",
        true,
    ),
    Case::trap(
        "value_unowned_dead",
        "panic: 'unowned' reference used after its target was destroyed",
        true,
    ),
];
