#include "dream_platform_internal.h"
#include "dream_thread.h"
#include "dream_core.h"
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <sys/mman.h>
#include <unistd.h>
#endif
#ifdef __APPLE__
#include <os/lock.h>
/* The ownership domains never wait on a condition and are taken several times per tracked
 * allocation, where a pthread mutex costs more than the work it guards. */
static os_unfair_lock locks[2] = {OS_UNFAIR_LOCK_INIT, OS_UNFAIR_LOCK_INIT};
static void platform_lock(unsigned domain) { os_unfair_lock_lock(&locks[domain]); }
static void platform_unlock(unsigned domain) { os_unfair_lock_unlock(&locks[domain]); }
#else
static dream_mutex locks[2] = {DREAM_MUTEX_INIT, DREAM_MUTEX_INIT};
static void platform_lock(unsigned domain) { dream_mutex_lock(&locks[domain]); }
static void platform_unlock(unsigned domain) { dream_mutex_unlock(&locks[domain]); }
#endif
static void *platform_map(size_t size) {
#ifdef _WIN32
    return VirtualAlloc(NULL, size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
#else
    void *p = mmap(NULL, size, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    return p == MAP_FAILED ? NULL : p;
#endif
}
static size_t page_size(void) {
#ifdef _WIN32
    SYSTEM_INFO info;
    GetSystemInfo(&info);
    return (size_t)info.dwPageSize;
#else
    long size = sysconf(_SC_PAGESIZE);
    return size > 0 ? (size_t)size : 4096u;
#endif
}
static size_t platform_release(void *ptr, size_t size) {
    static size_t page;
    if (page == 0) { page = page_size(); }
    uintptr_t start = ((uintptr_t)ptr + page - 1) & ~(uintptr_t)(page - 1);
    uintptr_t end = ((uintptr_t)ptr + size) & ~(uintptr_t)(page - 1);
    if (end <= start) { return 0; }
    size_t len = (size_t)(end - start);
#ifdef _WIN32
    if (VirtualAlloc((void *)start, len, MEM_RESET, PAGE_READWRITE) == NULL) { return 0; }
    /* MEM_RESET only drops the pages' contents; unlocking unlocked pages trims them from the
     * working set, which is what returns the memory. */
    (void)VirtualUnlock((void *)start, len);
#elif defined(__APPLE__)
    if (madvise((void *)start, len, MADV_FREE) != 0) { return 0; }
#else
    if (madvise((void *)start, len, MADV_DONTNEED) != 0) { return 0; }
#endif
    return len;
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
    malloc, realloc, free, platform_map, platform_release, abort, platform_write, platform_lock, platform_unlock, platform_object_drop
};
