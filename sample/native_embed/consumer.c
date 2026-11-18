#include "native_embed.h"

#include <stdint.h>
#include <stdio.h>

int main(void) {
    if (dream_embed_abi_version() != DREAM_EMBED_ABI_VERSION) {
        return 1;
    }

    dream_thread_attach();
    int32_t sum = add(20, 22);
    void *text = greeting();
    int32_t length = dream_string_length(text);
    int valid = sum == 42 && length == 5 && dream_string_units(text)[0] == 'D';
    dream_release(text);
    dream_thread_detach();

    if (!valid) {
        return 1;
    }
    printf("%d\n", (int)sum);
    return 0;
}
