#include "dream_host_support.h"
#include "dream_platform_internal.h"
#include <assert.h>
#include <stdlib.h>
#include <string.h>

const dream_platform *dream_platform_current;
char *dream_str_utf8(dream_ptr s) {
    (void)s;
    assert(!"printing must not allocate a full UTF-8 copy");
    abort();
}
dream_ptr dream_str_from_utf8(const char *s) {
    (void)s;
    assert(!"fixture does not read input");
    abort();
}
void print_string(dream_ptr s);
void print_err_string(dream_ptr s);

int main(int argc, char **argv) {
    assert(argc == 2);
    struct { int32_t len, pad; uint16_t units[1030]; } text = {1030, 0, {0}};
    for (int i = 0; i < 1022; i++) { text.units[i] = 'x'; }
    text.units[1022] = 0xd83d;
    text.units[1023] = 0xde00;
    text.units[1024] = 0xd800;
    /* Keep the surrogates lone, and verify bytes after the embedded zero stay hidden. */
    text.units[1025] = 'y';
    text.units[1026] = 0xdc00;
    text.units[1027] = 0xe9;
    text.units[1028] = 0;
    text.units[1029] = '!';
    dream_ptr s = (dream_ptr)(uintptr_t)&text;
    if (strcmp(argv[1], "stdout") == 0) { print_string(s); }
    else if (strcmp(argv[1], "stderr") == 0) { print_err_string(s); }
    else { abort(); }
    return 0;
}
