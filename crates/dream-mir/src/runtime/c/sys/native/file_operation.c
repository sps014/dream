#include "dream_host_support.h"
#include "dream_blocking.h"
#include "dream_file.h"
#include "dream_thread.h"
#include <stdlib.h>
#include <string.h>

/* Only host-owned copies enter the pool. A second reference outlives an abandoned guest
 * future; open results are closed unless explicitly claimed on the scheduler thread. */
typedef struct {
    dream_mutex mu;
    unsigned references;
    int32_t kind, fd, count, done, claimed;
    int64_t value, position;
    char *path, *mode;
    uint8_t *bytes;
    dream_ptr waiter;
} FileOperation;

static void operation_drop(FileOperation *op) {
    if (!op) { return; }
    dream_mutex_lock(&op->mu);
    unsigned references = --op->references;
    dream_mutex_unlock(&op->mu);
    if (references) { return; }
    if (op->kind == 0 && op->done && op->value >= 0 && !op->claimed) {
        dream_file_close((int32_t)op->value);
    }
    free(op->path);
    free(op->mode);
    free(op->bytes);
    dream_mutex_destroy(&op->mu);
    free(op);
}

static void operation_run(void *context) {
    FileOperation *op = context;
    int64_t value = -1;
    if (op->kind == 0) { value = dream_file_open(op->path, op->mode); }
    else if (op->fd >= 0) {
        if (op->kind == 1) { value = dream_file_read(op->fd, op->bytes, op->count); }
        else if (op->kind == 2) { value = dream_file_write(op->fd, op->bytes, op->count); }
        else if (op->kind == 3) { value = dream_file_seek(op->fd, op->position); }
        dream_file_close(op->fd);
    }
    dream_mutex_lock(&op->mu);
    op->value = value;
    op->done = 1;
    dream_ptr waiter = op->waiter;
    op->waiter = 0;
    dream_mutex_unlock(&op->mu);
    if (waiter) { dream_complete_foreign(waiter, 0); }
    operation_drop(op);
}

uintptr_t fileOperationStart(int32_t kind, int32_t fd, dream_ptr path, dream_ptr mode,
                             int32_t count, dream_ptr data, int64_t position) {
    if (kind < 0 || kind > 3 || count < 0 || count == INT32_MAX) { return 0; }
    FileOperation *op = calloc(1, sizeof(*op));
    if (!op) { return 0; }
    dream_mutex_init(&op->mu);
    op->references = 2;
    op->kind = kind;
    op->fd = -1;
    op->count = count;
    op->value = -1;
    op->position = position;
    if (kind == 0) {
        op->path = dream_str_utf8(path);
        op->mode = dream_str_utf8(mode);
    } else {
        op->fd = dream_file_duplicate(fd);
        if (kind == 1 || kind == 2) {
            op->bytes = malloc(count ? (size_t)count : 1);
            if (op->bytes && kind == 2 && count) {
                memcpy(op->bytes, (uint8_t *)dream_p(data) + 4, (size_t)count);
            }
        }
    }
    int valid = kind == 0 ? op->path && op->mode
        : op->fd >= 0 && (kind == 3 || op->bytes);
    if (!valid || !dream_blocking_submit(operation_run, op)) {
        if (op->fd >= 0) { dream_file_close(op->fd); }
        op->references = 1;
        operation_drop(op);
        return 0;
    }
    return (uintptr_t)op;
}

int32_t fileOperationDone(uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    if (!op) { return 1; }
    dream_mutex_lock(&op->mu);
    int done = op->done;
    dream_mutex_unlock(&op->mu);
    return done;
}

int64_t fileOperationResult(uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    if (!op) { return -1; }
    dream_mutex_lock(&op->mu);
    int64_t value = op->done ? op->value : -1;
    if (op->done) { op->claimed = 1; }
    dream_mutex_unlock(&op->mu);
    return value;
}

dream_ptr fileOperationBytes(uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    int64_t count = -1;
    if (op) {
        dream_mutex_lock(&op->mu);
        if (op->done && op->kind == 1) { count = op->value; }
        dream_mutex_unlock(&op->mu);
    }
    dream_ptr bytes = dream_array_new(count < 0 ? 1 : (int32_t)count + 1, 1);
    uint8_t *payload = (uint8_t *)dream_p(bytes) + 4;
    payload[0] = count < 0 ? 1 : 0;
    if (count > 0) { memcpy(payload + 1, op->bytes, (size_t)count); }
    return bytes;
}

void fileOperationRelease(uintptr_t handle) { operation_drop((FileOperation *)handle); }

int32_t fileOperationWaitAsync(dream_ptr future, uintptr_t handle) {
    FileOperation *op = (FileOperation *)handle;
    if (!op) { dream_async_complete(future, 0); return 0; }
    dream_mutex_lock(&op->mu);
    if (op->done) {
        dream_mutex_unlock(&op->mu);
        dream_async_complete(future, 0);
        return 0;
    }
    op->waiter = future;
    dream_mutex_unlock(&op->mu);
    return 1;
}
