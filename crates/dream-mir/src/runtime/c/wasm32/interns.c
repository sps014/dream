/* Interned-string getters and Debug counters the guest ABI imports (`dream_rt.h`).
 * The old WAT backend emitted these as module globals; here they are lazily built on
 * the Dream heap and cached for the process lifetime. */
#include "dream_rt_wasm32.h"

extern int32_t debug_get_free_list_head(void);
extern int32_t debug_get_live_objects(void);
extern int32_t debug_get_total_allocations(void);

static dream_ptr interned[4]; /* empty, true, false, minus */

extern int32_t live_objects;

/* Callers release these strings through ordinary ARC, so cached singletons must be
 * immortal (rc == INT32_MAX is ignored by retain/release) or the first release frees
 * them out from under later readers. Immortals also leave the live-object counter. */
static void pin_immortal(dream_ptr s) {
    if (s) {
        ((int32_t *)dream_p(s))[-1] = INT32_MAX;
        if (live_objects > 0) {
            live_objects -= 1;
        }
    }
}

static dream_ptr intern_latin1(const char *text) {
    dream_ptr s;
    int32_t n;
    uint16_t *units;
    for (n = 0; text[n]; n++) {
    }
    s = dream_string_alloc(n);
    units = (uint16_t *)dream_p(s + (int32_t)STRING_UNITS_OFFSET);
    for (int32_t i = 0; i < n; i++) {
        units[i] = (uint16_t)(unsigned char)text[i];
    }
    pin_immortal(s);
    return s;
}

static int32_t intern_slot(int32_t slot, const char *text) {
    if (!interned[slot]) {
        interned[slot] = intern_latin1(text);
    }
    return (int32_t)interned[slot];
}

int32_t intern_empty(void) { return intern_slot(0, ""); }
int32_t intern_true(void) { return intern_slot(1, "true"); }
int32_t intern_false(void) { return intern_slot(2, "false"); }
int32_t intern_minus(void) { return intern_slot(3, "-"); }

int32_t wasm_free_list_head(void) { return debug_get_free_list_head(); }
int32_t wasm_live_objects(void) { return debug_get_live_objects(); }
int32_t wasm_total_allocations(void) { return debug_get_total_allocations(); }

/* Old WAT-ABI shims: `dream_guest.h` declares these as env imports ("malloc"/"free");
 * providing definitions here turns them into plain internal calls. */
int32_t malloc_tagged(int32_t size, int32_t tag) {
    return (int32_t)dream_malloc(size, tag);
}

void free_tagged(int32_t ptr) {
    dream_free((dream_ptr)ptr);
}
