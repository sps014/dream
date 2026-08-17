#include "dream_rt.h"

IMPORT("malloc") int32_t rt_malloc(int32_t size, int32_t tag);
IMPORT("concat_strings") int32_t rt_concat_strings(int32_t a, int32_t b);
IMPORT("long_to_string") int32_t rt_long_to_string(int64_t v);

EXPORT("double_to_string")
int32_t double_to_string(double v) {
    int32_t neg = 0;
    int64_t micro;
    int64_t ip;
    int64_t fr;
    int32_t ipstr;
    int32_t buf;
    int32_t i;
    int32_t res;
    if (v < 0) {
        neg = 1;
        v = -v;
    }
    micro = (int64_t)(v * 1000000.0 + 0.5);
    ip = micro / 1000000;
    fr = micro % 1000000;
    ipstr = rt_long_to_string(ip);
    buf = rt_malloc(24, TAG_STRING);
    u16_store(buf + 8, 46);
    i = 6;
    while (i >= 1) {
        u16_store(buf + 8 + (i << 1), (uint16_t)((int32_t)(fr % 10) + 48));
        fr = fr / 10;
        i = i - 1;
    }
    i = 7;
    while (i > 1) {
        if (u16_load(buf + 8 + ((i - 1) << 1)) != 48) {
            break;
        }
        i = i - 1;
    }
    if (i == 1) {
        i = 0;
    }
    i32_store(buf, i);
    i32_store(buf + 4, 0);
    res = rt_concat_strings(ipstr, buf);
    if (neg) {
        res = rt_concat_strings(__rt_str_minus, res);
    }
    return res;
}

EXPORT("float_to_string")
int32_t float_to_string(float v) {
    return double_to_string((double)v);
}
