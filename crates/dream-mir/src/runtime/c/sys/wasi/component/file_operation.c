#include "dream_host_support.h"
#include "dream_file.h"
#include <stdlib.h>
#include <string.h>

/* WASI libc owns descriptor and pollable/future/stream handles. No foreign thread or guest
 * pointer survives an IO call, so cancellation can release an unclaimed result immediately. */
typedef struct {
    int32_t kind, claimed;
    int64_t value;
    uint8_t *bytes;
} FileOperation;

uintptr_t fileOperationStart(int32_t kind, int32_t fd, dream_ptr path, dream_ptr mode,
                             int32_t count, dream_ptr data, int64_t position) {
    if (kind < 0 || kind > 3 || count < 0 || count == INT32_MAX) { return 0; }
    FileOperation *op = calloc(1, sizeof(*op));
    if (!op) { return 0; }
    op->kind = kind;
    op->value = -1;
    if (kind == 0) {
        char *p = dream_str_utf8(path), *m = dream_str_utf8(mode);
        op->value = dream_file_open(p, m);
        free(p); free(m);
    } else if (kind == 3) { op->value = dream_file_seek(fd, position); }
    else if (kind == 1) {
        op->bytes = malloc(count ? (size_t)count : 1);
        if (op->bytes) { op->value = dream_file_read(fd, op->bytes, count); }
    } else {
        int32_t length = data ? dream_i32(data)[0] : 0;
        if (count <= length) { op->value = dream_file_write(fd, data ? (uint8_t *)dream_p(data) + 4 : NULL, count); }
    }
    return (uintptr_t)op;
}

int32_t fileOperationDone(uintptr_t handle) { (void)handle; return 1; }
int64_t fileOperationResult(uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    if (!op) { return -1; }
    op->claimed = 1;
    return op->value;
}

dream_ptr fileOperationBytes(uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    int64_t count = op && op->kind == 1 ? op->value : -1;
    dream_ptr result = dream_array_new(count < 0 ? 1 : (int32_t)count + 1, 1);
    uint8_t *payload = (uint8_t *)dream_p(result) + 4;
    payload[0] = count < 0 ? 1 : 0;
    if (count > 0) { memcpy(payload + 1, op->bytes, (size_t)count); }
    return result;
}

void fileOperationRelease(uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    if (!op) { return; }
    if (op->kind == 0 && op->value >= 0 && !op->claimed) { dream_file_close((int32_t)op->value); }
    free(op->bytes);
    free(op);
}

void fileOperationWait(dream_ptr future, uintptr_t handle) {
    (void)handle;
    dream_async_complete(future, 0);
}
