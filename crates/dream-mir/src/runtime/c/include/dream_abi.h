#ifndef DREAM_ABI_H
#define DREAM_ABI_H

/* Numeric guest ABI. Must stay in lockstep with crates/dream-mir/src/abi.rs
 * (enforced by `dream_abi_h_matches_abi_rs`). */

#define TAG_INT 1
#define TAG_FLOAT 2
#define TAG_DOUBLE 3
#define TAG_BOOL 4
#define TAG_STRING 5
#define TAG_ARRAY 6
#define TAG_CHAR 7
#define TAG_LONG 8
#define TAG_UINT 9
#define TAG_ULONG 10
#define TAG_BYTE 11
#define TAG_STRUCT_BASE 12

#define HEAP_HEADER_SIZE 12
#define HEADER_TAG_OFFSET 4
#define HEADER_REFCOUNT_OFFSET 8
#define LEN_PREFIX_SIZE 4
#define STRING_HEADER_SIZE 8
#define STRING_UNITS_OFFSET 8
#define STRING_SCALAR_LEN_OFFSET 4
#define DREAM_STR_PAD_INLINE 0
#define DREAM_STR_SLICE 1
#define NATIVE_HEAP_HEADER_SIZE 16
/* Written into cleared `unowned` slots (weak-registry kind 1) so a later load can tell
 * "target was destroyed" apart from "never assigned" (null). Must not be a valid pointer. */
#define DREAM_UNOWNED_POISON (-165764356) /* 0xF601A0CC as i32 */
#define RC_FROM_DATA 4
#define TAG_FROM_DATA 8

#define WASM_PAGE_SIZE 65536
#define SHADOW_STACK_SIZE (16 * WASM_PAGE_SIZE)
#define INITIAL_HEAP_PAGES 1
#define MAX_MEMORY_PAGES 65536
#define STRING_BASE 1024

#define DREAM_REGEX_IGNORE_CASE 2
#define DREAM_REGEX_MULTILINE 4
#define DREAM_REGEX_DOTALL 8

#define ALLOC_LOCK_ADDR 44
#define HEAP_PTR_ADDR 48
#define THREAD_ID_COUNTER_ADDR 52
#define ASYNC_RQ_HEAD_ADDR 76
#define ASYNC_RQ_TAIL_ADDR 80
#define ASYNC_TIMER_HEAD_ADDR 84
#define ASYNC_VCLOCK_ADDR 88

#define HEADER_LOCK_WORD_SIZE 4
#define LOCK_DEPTH_BITS 16

#define FUTURE_KIND_TASK 0
#define FUTURE_KIND_HOST 1
#define FUTURE_KIND_ALL 2
#define FUTURE_KIND_ANY 3
#define FUTURE_STATUS_PENDING 0
#define FUTURE_STATUS_READY 1
#define FUTURE_STATUS_CANCELLED 2
#define HOST_POLL_INDEX (-1)

#define F_STATE_WASM 0
#define F_STATUS_WASM 4
#define F_RESULT_WASM 8
#define F_POLL_WASM 12
#define F_WAKER_WASM 16
#define F_AWAITING_WASM 20
#define F_KIND_WASM 24
#define F_CHILDREN_WASM 28
#define F_COUNT_WASM 32
#define F_REMAINING_WASM 36
#define F_RESULTS_WASM 40
#define F_NEXT_WASM 44
#define F_QUEUED_WASM 48
#define F_DUE_WASM 52
#define F_WIDE_WASM 56
#define F_ESIZE_WASM F_WIDE_WASM
#define F_SLOTS_WASM 64

#define DREAM_SYM_MALLOC "malloc"
#define DREAM_SYM_FREE "free"
#define DREAM_SYM_MEMORY "memory"
#define DREAM_SYM_RUN_LOOP "__dream_run_loop"
#define DREAM_SYM_NEW_FUTURE "__dream_new_future"
#define DREAM_SYM_RESOLVE "__dream_resolve"
#define DREAM_SYM_RUNTIME_INIT "__runtime_init"
#define DREAM_MODULE_ENV "env"
#define DREAM_MODULE_HOST "Dream"
#define DREAM_SYM_PRINT_INT "print_int"
#define DREAM_SYM_PRINT_STRING "print_string"
#define DREAM_SYM_PRINT_CHAR "print_char"
#define DREAM_SYM_PRINT_FLOAT "print_float"
#define DREAM_SYM_PRINT_DOUBLE "print_double"
#define DREAM_SYM_TIME_NOW_NANOS "timeNowNanos"

#define F_STATE_NATIVE 0
#define F_STATUS_NATIVE 4
#define F_RESULT_NATIVE 8
#define F_POLL_NATIVE 16
#define F_WAKER_NATIVE 24
#define F_AWAITING_NATIVE 32
#define F_KIND_NATIVE 40
#define F_CHILDREN_NATIVE 48
#define F_COUNT_NATIVE 56
#define F_REMAINING_NATIVE 60
#define F_RESULTS_NATIVE 64
#define F_NEXT_NATIVE 72
#define F_QUEUED_NATIVE 80
#define F_DUE_NATIVE 84
#define F_ESIZE_NATIVE 88
#define F_WIDE_NATIVE 96
#define F_SLOTS_NATIVE 104

#ifdef DREAM_NATIVE
#define F_STATE F_STATE_NATIVE
#define F_STATUS F_STATUS_NATIVE
#define F_RESULT F_RESULT_NATIVE
#define F_POLL F_POLL_NATIVE
#define F_WAKER F_WAKER_NATIVE
#define F_AWAITING F_AWAITING_NATIVE
#define F_KIND F_KIND_NATIVE
#define F_CHILDREN F_CHILDREN_NATIVE
#define F_COUNT F_COUNT_NATIVE
#define F_REMAINING F_REMAINING_NATIVE
#define F_RESULTS F_RESULTS_NATIVE
#define F_NEXT F_NEXT_NATIVE
#define F_QUEUED F_QUEUED_NATIVE
#define F_DUE F_DUE_NATIVE
#define F_ESIZE F_ESIZE_NATIVE
#define F_WIDE F_WIDE_NATIVE
#define F_SLOTS F_SLOTS_NATIVE
#else
#define F_STATE F_STATE_WASM
#define F_STATUS F_STATUS_WASM
#define F_RESULT F_RESULT_WASM
#define F_POLL F_POLL_WASM
#define F_WAKER F_WAKER_WASM
#define F_AWAITING F_AWAITING_WASM
#define F_KIND F_KIND_WASM
#define F_CHILDREN F_CHILDREN_WASM
#define F_COUNT F_COUNT_WASM
#define F_REMAINING F_REMAINING_WASM
#define F_RESULTS F_RESULTS_WASM
#define F_NEXT F_NEXT_WASM
#define F_QUEUED F_QUEUED_WASM
#define F_DUE F_DUE_WASM
#define F_WIDE F_WIDE_WASM
#define F_ESIZE F_ESIZE_WASM
#define F_SLOTS F_SLOTS_WASM
#endif

#endif
