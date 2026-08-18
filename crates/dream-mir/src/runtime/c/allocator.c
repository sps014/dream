#include "dream_rt.h"

#ifdef NEEDS_THREADS
IMPORT("__alloc_lock_acquire") void rt_alloc_lock_acquire(void);
IMPORT("__alloc_lock_release") void rt_alloc_lock_release(void);
#endif

IMPORT("string_fini") void rt_string_fini(int32_t ptr);

EXPORT("size_class")
int32_t size_class(int32_t size) {
    int32_t s = size;
    if (s < 16) {
        s = 16;
    }
    return 28 - __builtin_clz((unsigned)(s - 1));
}

EXPORT("freelist_head_addr")
int32_t freelist_head_addr(int32_t idx) {
    if (idx <= 8) {
        return 4 + (idx << 2);
    }
    if (idx <= 12) {
        return 56 + ((idx - 9) << 2);
    }
    return 72;
}

EXPORT("__malloc_locked")
int32_t __malloc_locked(int32_t size, int32_t tag) {
    int32_t idx;
    int32_t alloc_size = 0;
    int32_t head_addr;
    int32_t block = 0;
    int32_t next;
    int32_t curr;
    int32_t prev;
    int32_t block_size;
    int32_t new_heap;
#ifdef DEBUG_ALLOC
    live_objects = live_objects + 1;
    total_allocations = total_allocations + 1;
#endif
    size = ((size + 3) & -4) + 12;
    idx = size_class(size);
    if (idx > 12) {
        alloc_size = size;
        head_addr = 72;
        curr = i32_load(head_addr);
        prev = 0;
        while (curr != 0) {
            block_size = i32_load(curr);
            if (block_size >= size) {
                next = i32_load(curr + 4);
                if (prev == 0) {
                    i32_store(head_addr, next);
                } else {
                    i32_store(prev + 4, next);
                }
                block = curr;
                break;
            }
            prev = curr;
            curr = i32_load(curr + 4);
        }
    } else {
        alloc_size = 1 << (idx + 4);
        head_addr = freelist_head_addr(idx);
        block = i32_load(head_addr);
        if (block != 0) {
            next = i32_load(block + 4);
            i32_store(head_addr, next);
        }
    }
    if (block == 0) {
        block = atomic_load_i32((int32_t)HEAP_PTR_ADDR);
        new_heap = block + alloc_size;
#ifdef __wasm__
        {
            int32_t mapped = wasm_memory_size() << 16;
            if ((uint32_t)new_heap > (uint32_t)mapped) {
                int32_t needed = ((new_heap - 1) >> 16) + 1;
                int32_t grow = needed - wasm_memory_size();
                if (wasm_memory_grow(grow) == -1) {
                    __builtin_unreachable();
                }
            }
        }
#endif
        atomic_store_i32((int32_t)HEAP_PTR_ADDR, new_heap);
        i32_store(block, alloc_size);
    }
    i32_store(block + 4, tag);
    i32_store(block + 8, 1);
    return block + 12;
}

EXPORT("malloc")
int32_t malloc_dream(int32_t size, int32_t tag) {
    int32_t result;
#ifdef NEEDS_THREADS
    rt_alloc_lock_acquire();
#endif
    result = __malloc_locked(size, tag);
#ifdef NEEDS_THREADS
    rt_alloc_lock_release();
#endif
    return result;
}

EXPORT("__free_locked")
void __free_locked(int32_t ptr) {
    int32_t block_start;
    int32_t idx;
    int32_t head_addr;
    int32_t size;
    if (ptr == 0) {
        return;
    }
    block_start = ptr - 12;
    size = i32_load(block_start);
    if (size == 0) {
        return;
    }
#ifdef DEBUG_ALLOC
    live_objects = live_objects - 1;
#endif
    idx = size_class(size);
    head_addr = freelist_head_addr(idx);
    i32_store(block_start + 4, i32_load(head_addr));
    i32_store(head_addr, block_start);
    free_list_head = block_start;
}

EXPORT("free")
void free_dream(int32_t ptr) {
    if (ptr == 0) {
        return;
    }
    rt_string_fini(ptr);
#ifdef NEEDS_THREADS
    rt_alloc_lock_acquire();
#endif
    __free_locked(ptr);
#ifdef NEEDS_THREADS
    rt_alloc_lock_release();
#endif
}

EXPORT("realloc")
int32_t realloc_dream(int32_t ptr, int32_t new_size, int32_t tag) {
    int32_t block_start;
    int32_t old_total;
    int32_t new_total;
    int32_t new_ptr;
    int32_t old_payload;
    int32_t copy_size;
    if (ptr == 0) {
        return malloc_dream(new_size, tag);
    }
    block_start = ptr - 12;
    old_total = i32_load(block_start);
    new_total = ((new_size + 3) & -4) + 12;
    if ((uint32_t)new_total <= (uint32_t)old_total) {
        return ptr;
    }
    new_ptr = malloc_dream(new_size, tag);
    old_payload = old_total - 12;
    copy_size = (uint32_t)old_payload < (uint32_t)new_size ? old_payload : new_size;
    mem_copy(new_ptr, ptr, copy_size);
    free_dream(ptr);
    return new_ptr;
}

EXPORT("retain")
void retain(int32_t ptr) {
    if (ptr == 0) {
        return;
    }
    atomic_fetch_add_i32(ptr - 4, 1);
}

EXPORT("object_tag")
int32_t object_tag(int32_t ptr) {
    if (ptr == 0) {
        return 0;
    }
    return i32_load(ptr - 8);
}

EXPORT("release_generic")
void release_generic(int32_t ptr) {
    int32_t old;
    if (ptr == 0) {
        return;
    }
    old = atomic_fetch_sub_i32(ptr - 4, 1);
    if (old == 1) {
        free_dream(ptr);
    }
}
