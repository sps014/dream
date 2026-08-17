#include "dream_rt.h"

IMPORT("malloc") int32_t rt_malloc(int32_t size, int32_t tag);
IMPORT("free") void rt_free(int32_t ptr);
IMPORT("retain") void rt_retain(int32_t ptr);
IMPORT("release_object") void rt_release_object(int32_t ptr);

EXPORT("funcbox_new")
int32_t funcbox_new(int32_t funcidx, int32_t env) {
    int32_t box = rt_malloc(8, 0);
    i32_store(box, funcidx);
    i32_store(box + 4, env);
    if (env != 0) {
        rt_retain(env);
    }
    return box;
}

EXPORT("funcbox_funcidx")
int32_t funcbox_funcidx(int32_t box) {
    return i32_load(box);
}

EXPORT("funcbox_env")
int32_t funcbox_env(int32_t box) {
    return i32_load(box + 4);
}

EXPORT("release_funcbox")
void release_funcbox(int32_t ptr) {
    int32_t rc;
    int32_t nc;
    int32_t env;
    if (ptr == 0) {
        return;
    }
    rc = ptr - 4;
    nc = i32_load(rc) - 1;
    i32_store(rc, nc);
    if (nc == 0) {
        env = i32_load(ptr + 4);
        if (env != 0) {
            rt_release_object(env);
        }
        rt_free(ptr);
    }
}
