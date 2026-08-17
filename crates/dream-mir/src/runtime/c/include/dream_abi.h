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
#define STRING_UTF8_OFFSET 8
#define STRING_SCALAR_LEN_OFFSET 4

#define WASM_PAGE_SIZE 65536
#define SHADOW_STACK_SIZE (16 * WASM_PAGE_SIZE)
#define INITIAL_HEAP_PAGES 1
#define MAX_MEMORY_PAGES 65536
#define STRING_BASE 1024

#define ALLOC_LOCK_ADDR 44
#define HEAP_PTR_ADDR 48
#define THREAD_ID_COUNTER_ADDR 52
#define ASYNC_RQ_HEAD_ADDR 76
#define ASYNC_RQ_TAIL_ADDR 80
#define ASYNC_TIMER_HEAD_ADDR 84
#define ASYNC_VCLOCK_ADDR 88

#define HEADER_LOCK_WORD_SIZE 4
#define LOCK_DEPTH_BITS 16

#endif
