#include "include/dream_rt_native.h"

#include <stdio.h>
#include <stdlib.h>

void dream_panic(dream_ptr msg) {
    print_string(msg);
    print_char(10);
    abort();
}
