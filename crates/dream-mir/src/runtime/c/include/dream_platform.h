#ifndef DREAM_PLATFORM_H
#define DREAM_PLATFORM_H
#include <stddef.h>
/* Supply before any runtime use; retain the table and allocator for the process lifetime.
 * map returns zero-filled, 16-byte-aligned storage. lock domains may nest (weak then heap).
 * abort must never return. write receives stream 1 (stdout) or 2 (stderr), a span and its encoding.
 * Span length counts bytes for UTF-8, code units for UTF-16 (native endian). */
typedef struct dream_platform {
    void *(*allocate)(size_t);
    void *(*resize)(void *, size_t);
    void (*deallocate)(void *);
    void *(*map)(size_t);
    void (*abort)(void);
    void (*write)(int, const void *, size_t, int);
    void (*lock)(unsigned);
    void (*unlock)(unsigned);
    void (*object_drop)(void *);
} dream_platform;
enum { DREAM_LOCK_HEAP, DREAM_LOCK_WEAK };
enum { DREAM_TEXT_UTF8, DREAM_TEXT_UTF16 };
#endif
