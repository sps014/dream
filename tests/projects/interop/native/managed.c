#include <dream_embed.h>

#include <stdint.h>

void *take_bytes(int32_t n) {
    void *array = dream_bytes_new(n);
    unsigned char *bytes = (unsigned char *)dream_array_data(array);
    int32_t i;
    for (i = 0; i < n; i++) {
        bytes[i] = (unsigned char)(i + 1);
    }
    return array;
}

void *owned_hello(void) {
    return dream_string_from_utf8("hello", 5);
}

int32_t view_len(dream_str_view text) {
    return (int32_t)text.len;
}

int32_t add_later(void *future, int32_t a, int32_t b) {
    dream_future_complete_u64(future, (uint64_t)(a + b));
    return 0;
}
