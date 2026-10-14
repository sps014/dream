#include "dream_core.h"
#include "dream_thread.h"
#include "dream_file.h"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

static dream_mutex mu = DREAM_MUTEX_INIT;
static dream_cond changed = DREAM_COND_INIT;
static int live = 1, entered, released, wakes;

char *dream_str_utf8(dream_ptr p) {
    size_t size = strlen((char *)p) + 1;
    char *copy = malloc(size);
    assert(copy);
    memcpy(copy, p, size);
    return copy;
}
dream_ptr dream_array_new(int32_t count, int32_t size) {
    int32_t *p = calloc(1, 4 + (size_t)count * size);
    assert(p);
    *p = count;
    return (dream_ptr)p;
}
int32_t dream_file_open(const char *path, const char *mode) {
    assert(!strcmp(path, "file") && !strcmp(mode, "r"));
    dream_mutex_lock(&mu);
    ++live;
    dream_mutex_unlock(&mu);
    return 7;
}
int32_t dream_file_duplicate(int32_t fd) {
    assert(fd == 7);
    dream_mutex_lock(&mu);
    ++live;
    dream_mutex_unlock(&mu);
    return fd;
}
int32_t dream_file_close(int32_t fd) {
    assert(fd == 7);
    dream_mutex_lock(&mu);
    --live;
    dream_cond_broadcast(&changed);
    dream_mutex_unlock(&mu);
    return 0;
}
int64_t dream_file_read(int32_t fd, void *buffer, int32_t count) {
    assert(fd == 7 && count == 3);
    dream_mutex_lock(&mu);
    ++entered;
    dream_cond_broadcast(&changed);
    while (!released) { dream_cond_wait(&changed, &mu); }
    dream_mutex_unlock(&mu);
    memcpy(buffer, "ABC", 3);
    return 3;
}
int64_t dream_file_write(int32_t fd, const void *buffer, int32_t count) {
    assert(fd == 7 && count == 3 && !memcmp(buffer, "ABC", 3));
    return 2;
}
int32_t dream_file_seek(int32_t fd, int64_t position) {
    assert(fd == 7);
    return position < 0 ? -1 : 0;
}
void dream_async_complete(dream_ptr future, dream_result value) {
    assert(future && value == 0);
    dream_mutex_lock(&mu);
    ++wakes;
    dream_cond_broadcast(&changed);
    dream_mutex_unlock(&mu);
}
void dream_complete_foreign(dream_ptr future, dream_result value) { dream_async_complete(future, value); }
#include "../crates/dream-mir/src/runtime/c/sys/native/file_operation.c"

int main(void) {
    uintptr_t reads[4];
    for (unsigned i = 0; i < 4; ++i) {
        reads[i] = fileOperationStart(1, 7, 0, 0, 3, 0, 0);
        assert(reads[i]);
    }
    dream_mutex_lock(&mu);
    while (entered != 4) { dream_cond_wait(&changed, &mu); }
    dream_mutex_unlock(&mu);
    assert(fileOperationWaitAsync((dream_ptr)(uintptr_t)1, reads[0]) == 1);
    fileOperationRelease(reads[0]);
    uintptr_t orphan = fileOperationStart(0, -1, (dream_ptr)"file", (dream_ptr)"r", 0, 0, 0);
    assert(orphan);
    assert(fileOperationWaitAsync((dream_ptr)(uintptr_t)2, orphan) == 1);
    fileOperationRelease(orphan);
    dream_mutex_lock(&mu);
    assert(live == 5);
    released = 1;
    dream_cond_broadcast(&changed);
    while (wakes != 2 || live != 1) { dream_cond_wait(&changed, &mu); }
    dream_mutex_unlock(&mu);
    for (unsigned i = 1; i < 4; ++i) {
        while (!fileOperationDone(reads[i])) { dream_thread_yield(); }
        dream_ptr bytes = fileOperationBytes(reads[i]);
        assert(dream_i32(bytes)[0] == 4);
        assert(!memcmp((char *)bytes + 4, "\0ABC", 4));
        free(bytes);
        fileOperationRelease(reads[i]);
    }
    dream_ptr data = dream_array_new(3, 1);
    memcpy((char *)data + 4, "ABC", 3);
    uintptr_t write = fileOperationStart(2, 7, 0, 0, 3, data, 0);
    memset((char *)data + 4, 0, 3);
    free(data);
    while (!fileOperationDone(write)) { dream_thread_yield(); }
    assert(fileOperationResult(write) == 2);
    fileOperationRelease(write);
    uintptr_t open = fileOperationStart(0, -1, (dream_ptr)"file", (dream_ptr)"r", 0, 0, 0);
    while (!fileOperationDone(open)) { dream_thread_yield(); }
    assert(fileOperationResult(open) == 7);
    fileOperationRelease(open);
    dream_file_close(7);
    dream_mutex_lock(&mu);
    assert(live == 1);
    dream_mutex_unlock(&mu);
    puts("file operation ownership passed");
    return 0;
}
