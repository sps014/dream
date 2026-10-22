#include "async_frame.h"

#define KIND_ALL FUTURE_KIND_ALL
#define KIND_RACE FUTURE_KIND_RACE

typedef struct {
    void (*copy)(dream_ptr, dream_ptr);
    dream_result (*clone)(dream_result);
    const dream_type_info *array_info;
} CombinatorState;
static CombinatorState *combinator_state(dream_ptr f) {
    return (CombinatorState *)((char *)dream_p(f) + F_SLOTS);
}
void dream_combinator_progress(dream_ptr w, dream_ptr child) {
    int32_t kind = i32_at(w, F_KIND)[0];
    if (status_of(w)) { return; }
    if (kind != KIND_RACE && --i32_at(w, F_REMAINING)[0] > 0) { return; }
    int32_t n = i32_at(w, F_COUNT)[0];
    dream_ptr kids = ptr_at(w, F_CHILDREN)[0];
    for (int32_t i = 0; i < n; ++i) {
        dream_ptr c = arr_get(kids, i);
        if (c && ptr_at(c, F_WAKER)[0] == w) { ptr_at(c, F_WAKER)[0] = 0; }
    }
    CombinatorState *state = combinator_state(w);
    if (kind == KIND_RACE) {
        memcpy((char *)dream_p(w) + F_WIDE, (char *)dream_p(child) + F_WIDE, 8);
        dream_async_complete(w, state->clone(*result_at(child)));
    } else {
        int32_t es = i32_at(w, F_ESIZE)[0];
        dream_ptr out = dream_array_new(n, es);
        dream_set_type(out, state->array_info);
        for (int32_t i = 0; i < n; ++i) {
            dream_ptr c = arr_get(kids, i);
            if (c) { state->copy((dream_ptr)((char *)dream_p(out) + LEN_PREFIX_SIZE + (size_t)i * (size_t)es), c); }
        }
        dream_async_complete(w, (dream_result)(uintptr_t)out);
    }
}

static dream_ptr combinator_new(dream_ptr arr, int32_t kind, int32_t esize, void (*copy)(dream_ptr, dream_ptr), dream_result (*clone)(dream_result), const dream_type_info *array_info) {
    int32_t n = arr ? dream_i32(arr)[0] : 0;
    int32_t i;
    dream_ptr w = dream_new_future(F_SLOTS + sizeof(CombinatorState), HOST_POLL_INDEX, kind);
    *combinator_state(w) = (CombinatorState){copy, clone, array_info};
    dream_retain(arr);
    ptr_at(w, F_CHILDREN)[0] = arr;
    i32_at(w, F_COUNT)[0] = n;
    i32_at(w, F_REMAINING)[0] = n;
    i32_at(w, F_ESIZE)[0] = esize > 0 ? esize : 4;
    if (n == 0 && kind == KIND_ALL) {
        dream_ptr out = dream_array_new(0, esize);
        dream_set_type(out, array_info);
        dream_async_complete(w, (dream_result)(uintptr_t)out);
        return w;
    }
    for (i = 0; i < n; i++) {
        dream_ptr c = arr_get(arr, i);
        if (!c) {
            continue;
        }
        ptr_at(c, F_WAKER)[0] = w;
        if (status_of(c)) {
            dream_combinator_progress(w, c);
        }
    }
    return w;
}

dream_ptr dream_all(dream_ptr arr, int32_t esize, void (*copy)(dream_ptr, dream_ptr), const dream_type_info *info) { return combinator_new(arr, KIND_ALL, esize, copy, NULL, info); }

dream_ptr dream_race(dream_ptr arr, dream_result (*clone)(dream_result)) { return combinator_new(arr, KIND_RACE, 8, NULL, clone, NULL); }
