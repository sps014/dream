#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_heap_maps.h"

/* Large buffers use independent mappings and a process-wide first-fit list so their storage
 * remains reusable after the allocating thread exits. */
static dream_ptr large_freelist;
/* Free large blocks keep their pages up to this many bytes. Past it, blocks that stayed unclaimed
 * for LARGE_RELEASE_AGE hand their interior pages back to the OS. Wall-clock age decides, because
 * refaulting a buffer that a loop frees and reallocates costs far more than its idle pages. */
#define IDLE_LARGE_RETAIN ((size_t)16u << 20)
/* Ticks of 2^20 ns; 1024 ticks is about one second. */
#define LARGE_RELEASE_AGE ((uintptr_t)1024u)
/* A free large block's prefix word is either its free-time tick tagged LARGE_RESIDENT or the
 * page-aligned count of released bytes tagged LARGE_RELEASED. */
enum { LARGE_RESIDENT = 1, LARGE_RELEASED = 2 };
#define LARGE_TICK_MASK (UINTPTR_MAX >> 2)
static size_t idle_large_bytes;
static uint64_t released_bytes;
static uintptr_t large_swept_tick;

static uintptr_t large_tick(void) {
    return ((uintptr_t)((uint64_t)dream_platform_current->now() >> 20)) & LARGE_TICK_MASK;
}

/* Unlinks resident blocks idle for LARGE_RELEASE_AGE until the resident total fits the budget.
 * They stay off the list while their pages are released, so no thread can claim one mid-release. */
static char *take_stale_large_locked(uintptr_t now) {
    char *stale = NULL;
    dream_ptr prev = 0;
    dream_ptr curr = large_freelist;
    while (curr != 0 && idle_large_bytes > IDLE_LARGE_RETAIN) {
        char *block = (char *)dream_p(curr);
        dream_ptr next = (dream_ptr)dream_block_next(block);
        uintptr_t state = *dream_large_state(block);
        if ((state & LARGE_RESIDENT) && ((now - (state >> 2)) & LARGE_TICK_MASK) >= LARGE_RELEASE_AGE) {
            if (prev == 0) {
                large_freelist = next;
            } else {
                dream_block_link((char *)dream_p(prev), dream_p(next));
            }
            idle_large_bytes -= *dream_large_size(block);
            dream_block_link(block, stale);
            stale = block;
        } else {
            prev = curr;
        }
        curr = next;
    }
    return stale;
}

/* Releasing pages costs a system call and a fault on reuse, so it runs outside the heap lock and
 * at most once per LARGE_RELEASE_AGE. */
static void release_stale_large(char *stale) {
    char *head = stale;
    size_t released = 0;
    for (char *block = stale; block != NULL; block = dream_block_next(block)) {
        size_t sz = *dream_large_size(block);
        char *start = block + HEAP_HEADER_SIZE + 8;
        size_t length = sz - HEAP_HEADER_SIZE - 8;
        /* Injected release hooks may overwrite the span whose contents they discard. */
        DREAM_UNPOISON(start, length);
        size_t bytes = dream_platform_current->release(start, length);
        DREAM_POISON(start, length);
        *dream_large_state(block) = (uintptr_t)bytes | LARGE_RELEASED;
        released += bytes;
    }
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    released_bytes += released;
    /* First fit scans from the head, so resident blocks ahead of released ones are reused first. */
    if (large_freelist == 0) {
        large_freelist = (dream_ptr)head;
    } else {
        char *last = (char *)dream_p(large_freelist);
        while (dream_block_next(last) != NULL) {
            last = dream_block_next(last);
        }
        dream_block_link(last, head);
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
}

void dream_large_recycle(char *block) {
    char *stale = NULL;
    uintptr_t now = large_tick();
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    *dream_large_state(block) = (now << 2) | LARGE_RESIDENT;
    idle_large_bytes += *dream_large_size(block);
    dream_block_link(block, dream_p(large_freelist));
    large_freelist = (dream_ptr)block;
    if (idle_large_bytes > IDLE_LARGE_RETAIN && ((now - large_swept_tick) & LARGE_TICK_MASK) >= LARGE_RELEASE_AGE) {
        large_swept_tick = now;
        stale = take_stale_large_locked(now);
    }
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    if (stale != NULL) {
        release_stale_large(stale);
    }
}

static void claim_large(char *block) {
    uintptr_t state = *dream_large_state(block);
    if (state & LARGE_RELEASED) {
        released_bytes -= state & ~(uintptr_t)(LARGE_RESIDENT | LARGE_RELEASED);
    } else {
        idle_large_bytes -= *dream_large_size(block);
    }
    *dream_large_state(block) = 0;
}

uint64_t dream_heap_released_bytes(void) {
    dream_platform_current->lock(DREAM_LOCK_HEAP);
    uint64_t bytes = released_bytes;
    dream_platform_current->unlock(DREAM_LOCK_HEAP);
    return bytes;
}

char *dream_large_take_locked(size_t need) {
    dream_ptr prev = 0;
    dream_ptr curr = large_freelist;
    while (curr != 0) {
        char *block = (char *)dream_p(curr);
        void *next = dream_block_next(block);
        if (*dream_large_size(block) >= need) {
            if (prev == 0) {
                large_freelist = (dream_ptr)next;
            } else {
                dream_block_link((char *)dream_p(prev), next);
            }
            claim_large(block);
            return block;
        }
        prev = curr;
        curr = (dream_ptr)next;
    }
    return NULL;
}

