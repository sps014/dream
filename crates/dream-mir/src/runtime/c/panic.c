#include "dream_rt.h"

IMPORT("print_string") void rt_print_string(int32_t msg);
IMPORT("print_char") void rt_print_char(int32_t c);

EXPORT("dream_panic")
void dream_panic(int32_t msg) {
    rt_print_string(msg);
    rt_print_char(10);
    __builtin_unreachable();
}
