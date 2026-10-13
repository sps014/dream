use super::Case;

pub(super) const CASES: &[Case] = &[
    Case::trap(
        "priority_queue_reentrant_comparator",
        "PriorityQueue cannot be mutated by its comparator",
        true,
    ),
    Case::trap(
        "abc_post_loop_trap",
        "10\n20\n30\npanic: index out of bounds\nabc_post_loop_trap.dream:12",
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
        "arc_cycles_resurrection",
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
        "hash_capacity_limit_trap",
        "16\npanic: capacity exceeds the supported limit",
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
        "map_indexer_trap",
        "1\nMap key not found\nmap_indexer_trap.dream:10",
        true,
    ),
    Case::trap(
        "overflow_add_byte",
        "start\npanic: attempt to add with overflow",
        true,
    ),
    Case::trap(
        "overflow_add_int",
        "start\npanic: attempt to add with overflow\noverflow_add_int.dream:3",
        true,
    ),
    Case::trap(
        "overflow_checked_lambda",
        "start\npanic: attempt to multiply with overflow",
        true,
    ),
    Case::trap(
        "overflow_compound_add_int",
        "start\npanic: attempt to add with overflow",
        true,
    ),
    Case::trap(
        "overflow_const_add_int",
        "start\npanic: attempt to add with overflow",
        true,
    ),
    Case::trap(
        "overflow_div_int_min",
        "start\npanic: attempt to divide with overflow",
        true,
    ),
    Case::trap(
        "overflow_increment_uint",
        "start\npanic: attempt to add with overflow",
        true,
    ),
    Case::trap(
        "overflow_mul_long",
        "start\npanic: attempt to multiply with overflow",
        true,
    ),
    Case::trap(
        "overflow_mul_ulong",
        "start\npanic: attempt to multiply with overflow",
        true,
    ),
    Case::trap(
        "overflow_neg_int",
        "start\npanic: attempt to negate with overflow",
        true,
    ),
    Case::trap(
        "overflow_neg_long",
        "start\npanic: attempt to negate with overflow",
        true,
    ),
    Case::trap(
        "overflow_rem_long_min",
        "start\npanic: attempt to calculate the remainder with overflow",
        true,
    ),
    Case::trap(
        "overflow_shl_int",
        "start\npanic: attempt to shift left with overflow",
        true,
    ),
    Case::trap(
        "overflow_shr_long",
        "start\npanic: attempt to shift right with overflow",
        true,
    ),
    Case::trap(
        "overflow_sub_byte",
        "start\npanic: attempt to subtract with overflow",
        true,
    ),
    Case::trap(
        "overflow_sub_uint",
        "start\npanic: attempt to subtract with overflow",
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
        "panic_div_zero_unused",
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
        "read_only_span_text_slice_trap",
        "Span slice out of range\nexit code 1",
        true,
    ),
    Case::trap("span_bounds_trap", "3\nSpan index out of range", true),
    Case::trap(
        "span_range_trap",
        "2\nSpan range out of bounds\nspan_range_trap.dream:7",
        true,
    ),
    Case::trap(
        "string_builder_slice_oob",
        "StringBuilder slice out of bounds\nstring_builder_slice_oob.dream:4",
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
        "string_repeat_overflow_trap",
        "abcdabcd\npanic: size exceeds the supported limit",
        true,
    ),
    Case::trap(
        "string_scan_loop_oob_trap",
        "panic: index out of bounds\nstring_scan_loop_oob_trap.dream:10",
        true,
    ),
    Case::trap(
        "string_set_oob_trap",
        "129\npanic: index out of bounds",
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
