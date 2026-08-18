#include "include/dream_rt_native.h"

static dream_ptr box_bytes(const void *src, int32_t n, int32_t tag) {
    dream_ptr p = dream_malloc(n, tag);
    memcpy(dream_p(p), src, (size_t)n);
    return p;
}

dream_ptr dream_box_int(int32_t v) { return box_bytes(&v, 4, TAG_INT); }
dream_ptr dream_box_uint(int32_t v) { return box_bytes(&v, 4, TAG_UINT); }
dream_ptr dream_box_float(float v) { return box_bytes(&v, 4, TAG_FLOAT); }
dream_ptr dream_box_double(double v) { return box_bytes(&v, 8, TAG_DOUBLE); }
dream_ptr dream_box_bool(int32_t v) { return box_bytes(&v, 4, TAG_BOOL); }
dream_ptr dream_box_char(int32_t v) { return box_bytes(&v, 4, TAG_CHAR); }
dream_ptr dream_box_long(int64_t v) { return box_bytes(&v, 8, TAG_LONG); }
dream_ptr dream_box_ulong(int64_t v) { return box_bytes(&v, 8, TAG_ULONG); }
dream_ptr dream_box_byte(int32_t v) { return box_bytes(&v, 4, TAG_BYTE); }

int32_t dream_unbox_int(dream_ptr p) { return p ? *(int32_t *)dream_p(p) : 0; }
int32_t dream_unbox_uint(dream_ptr p) { return p ? *(int32_t *)dream_p(p) : 0; }
float dream_unbox_float(dream_ptr p) { return p ? *(float *)dream_p(p) : 0; }
double dream_unbox_double(dream_ptr p) { return p ? *(double *)dream_p(p) : 0; }
int32_t dream_unbox_bool(dream_ptr p) { return p ? *(int32_t *)dream_p(p) : 0; }
int32_t dream_unbox_char(dream_ptr p) { return p ? *(int32_t *)dream_p(p) : 0; }
int64_t dream_unbox_long(dream_ptr p) { return p ? *(int64_t *)dream_p(p) : 0; }
int64_t dream_unbox_ulong(dream_ptr p) { return p ? *(int64_t *)dream_p(p) : 0; }
int32_t dream_unbox_byte(dream_ptr p) { return p ? *(int32_t *)dream_p(p) : 0; }

int32_t dream_hash_value(dream_ptr p) {
    return dream_object_hash_code(p);
}

int32_t dream_object_hash_code(dream_ptr p) {
    if (!p) {
        return 0;
    }
    return (int32_t)((uintptr_t)p * 2654435761u);
}

dream_ptr dream_array_to_string(dream_ptr arr) {
    dream_ptr r = dream_string_alloc(1);
    uint16_t *u;
    (void)arr;
    dream_i32(r)[0] = 2;
    u = (uint16_t *)((char *)dream_p(r) + STRING_UTF8_OFFSET);
    u[0] = '[';
    u[1] = ']';
    return r;
}
