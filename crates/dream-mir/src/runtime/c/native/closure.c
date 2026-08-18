#include "include/dream_rt_native.h"

dream_ptr dream_funcbox_new(int32_t idx, dream_ptr env) {
    dream_ptr p = dream_malloc(16, TAG_STRUCT_BASE);
    dream_i32(p)[0] = idx;
    ((dream_ptr *)dream_p(p))[1] = env;
    dream_retain(env);
    return p;
}

int32_t dream_funcbox_funcidx(dream_ptr box) {
    return box ? dream_i32(box)[0] : 0;
}

dream_ptr dream_funcbox_env(dream_ptr box) {
    return box ? ((dream_ptr *)dream_p(box))[1] : 0;
}
