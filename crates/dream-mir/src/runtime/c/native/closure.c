#include "include/dream_rt_native.h"

/* Env sits at byte 8 so a 32-bit idx and env are not packed into one i64 store. Wasm32 heap
 * payloads are 4-mod-8 (`HEAP_HEADER_SIZE` 12), and an i64 store there traps. Native already
 * used offset 8 (`dream_ptr[1]`). */
enum { FUNCBOX_ENV_OFF = 8 };

static void funcbox_set_env(dream_ptr box, dream_ptr env) {
    memcpy((char *)dream_p(box) + FUNCBOX_ENV_OFF, &env, sizeof(env));
}

static dream_ptr funcbox_get_env(dream_ptr box) {
    dream_ptr env;
    memcpy(&env, (char *)dream_p(box) + FUNCBOX_ENV_OFF, sizeof(env));
    return env;
}

dream_ptr dream_funcbox_new(int32_t idx, dream_ptr env) {
    dream_ptr p = dream_malloc(16, TAG_STRUCT_BASE);
    memcpy(dream_p(p), &idx, sizeof(idx));
    funcbox_set_env(p, env);
    if (env) {
        dream_retain(env);
    }
    return p;
}

int32_t dream_funcbox_funcidx(dream_ptr box) {
    int32_t idx = 0;
    if (box) {
        memcpy(&idx, dream_p(box), sizeof(idx));
    }
    return idx;
}

dream_ptr dream_funcbox_env(dream_ptr box) {
    return box ? funcbox_get_env(box) : 0;
}

void dream_release_funcbox(dream_ptr box) {
    dream_ptr env;
    dream_ptr zero = 0;
    if (!box) {
        return;
    }
    if (!dream_rc_last(box)) {
        return;
    }
    env = funcbox_get_env(box);
    funcbox_set_env(box, zero);
    dream_release_object(env);
    dream_free(box);
}
