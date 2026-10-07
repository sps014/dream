#include "dream_platform_internal.h"
#include "dream_thread.h"
#include "dream_core.h"
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#endif
static dream_mutex locks[3] = {DREAM_MUTEX_INIT, DREAM_MUTEX_INIT, DREAM_MUTEX_INIT};
static void platform_lock(unsigned domain) { dream_mutex_lock(&locks[domain]); }
static void platform_unlock(unsigned domain) { dream_mutex_unlock(&locks[domain]); }
static void *platform_map(size_t size) {
#ifdef _WIN32
    return VirtualAlloc(NULL, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
#else
    void *p = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    return p == MAP_FAILED ? NULL : p;
#endif
}
static void platform_write(int stream, const void *bytes, size_t size, int encoding) {
    if (encoding == DREAM_TEXT_UTF16) {
        dream_write_utf16(stream, bytes, (int32_t)size);
        return;
    }
    FILE *file = stream == 2 ? stderr : stdout;
    fwrite(bytes, 1, size, file);
    fflush(file);
}
static void platform_object_drop(void *ptr) { dream_lock_forget((dream_ptr)ptr); }
const dream_platform dream_default_platform = {
    malloc, realloc, free, platform_map, abort, platform_write, platform_lock, platform_unlock, platform_object_drop
};
