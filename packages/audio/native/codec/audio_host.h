#ifndef AUDIO_HOST_H
#define AUDIO_HOST_H

#include <dream_embed.h>

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* Defined by the core runtime. Package codecs allocate through it on every target. */
extern const dream_platform *dream_platform_current;

static inline void *dream_raw_calloc(size_t count, size_t size) {
    if (size && count > SIZE_MAX / size) {
        return NULL;
    }
    size_t bytes = count * size;
    void *pointer = dream_platform_current->allocate(bytes);
    if (pointer && bytes) {
        memset(pointer, 0, bytes);
    }
    return pointer;
}

#endif
