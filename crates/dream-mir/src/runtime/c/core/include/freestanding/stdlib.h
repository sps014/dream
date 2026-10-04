#ifndef DREAM_FREESTANDING_STDLIB_H
#define DREAM_FREESTANDING_STDLIB_H
#include <stddef.h>
/* Declaration-only standard interfaces referenced by vendored uthash/utarray headers.
 * Live allocations are redirected to dream_platform; the freestanding gate rejects libc imports. */
void *malloc(size_t);
void *realloc(void *, size_t);
void free(void *);
_Noreturn void exit(int);
#endif
