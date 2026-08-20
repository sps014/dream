#include "dream_rt_wasm32.h"

int32_t dream_tid_get(void);
void dream_tid_set(int32_t v);

int32_t dream_instance_tid(void) {
    int32_t t = dream_tid_get();
    if (t == 0) {
        t = dream_next_tid();
        dream_tid_set(t);
    }
    return t;
}
