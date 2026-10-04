#include "dream_rt_wasm32.h"
#include "dream_platform_internal.h"
#include <wasi/api.h>

/* The WASI libc archive funnels these syscalls through imported entry points. Providing
 * them inside the guest keeps portable stdio independent of a second host runtime. */
int32_t __imported_wasi_snapshot_preview1_environ_sizes_get(int32_t count, int32_t size) {
    *(uint32_t *)(uintptr_t)(uint32_t)count = 0;
    *(uint32_t *)(uintptr_t)(uint32_t)size = 0;
    return 0;
}

int32_t __imported_wasi_snapshot_preview1_environ_get(int32_t entries, int32_t data) {
    (void)entries; (void)data;
    return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_write(int32_t fd, int32_t vectors, int32_t count, int32_t written) {
    if (fd != 1 && fd != 2) { return __WASI_ERRNO_BADF; }
    const __wasi_ciovec_t *iov = (const __wasi_ciovec_t *)(uintptr_t)(uint32_t)vectors;
    uint32_t total = 0;
    for (uint32_t i = 0; i < (uint32_t)count; ++i) {
        dream_platform_current->write(fd, iov[i].buf, iov[i].buf_len, 0);
        total += (uint32_t)iov[i].buf_len;
    }
    *(uint32_t *)(uintptr_t)(uint32_t)written = total;
    return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_get(int32_t fd, int32_t out) {
    if (fd < 0 || fd > 2) { return __WASI_ERRNO_BADF; }
    __wasi_fdstat_t *stat = (__wasi_fdstat_t *)(uintptr_t)(uint32_t)out;
    memset(stat, 0, sizeof(*stat));
    stat->fs_filetype = __WASI_FILETYPE_CHARACTER_DEVICE;
    stat->fs_rights_base = fd == 0 ? __WASI_RIGHTS_FD_READ : __WASI_RIGHTS_FD_WRITE;
    return 0;
}

int32_t __imported_wasi_snapshot_preview1_fd_close(int32_t fd) {
    (void)fd;
    return __WASI_ERRNO_BADF;
}

int32_t __imported_wasi_snapshot_preview1_fd_seek(int32_t fd, int64_t offset, int32_t whence, int32_t out) {
    (void)fd; (void)offset; (void)whence; (void)out;
    return __WASI_ERRNO_SPIPE;
}

_Noreturn void __imported_wasi_snapshot_preview1_proc_exit(int32_t code) {
    (void)code;
    dream_platform_abort();
}
