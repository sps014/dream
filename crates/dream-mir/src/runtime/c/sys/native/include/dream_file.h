#ifndef DREAM_FILE_H
#define DREAM_FILE_H
#include <stdint.h>

int32_t dream_file_open(const char *path, const char *mode);
int32_t dream_file_duplicate(int32_t fd);
int64_t dream_file_read(int32_t fd, void *buffer, int32_t count);
int64_t dream_file_write(int32_t fd, const void *buffer, int32_t count);
int32_t dream_file_seek(int32_t fd, int64_t position);
int32_t dream_file_close(int32_t fd);

/* fileOperationStart kinds; native runs them on a blocking worker, WASI inline. */
enum { DREAM_FILE_OPEN = 0, DREAM_FILE_READ = 1, DREAM_FILE_WRITE = 2, DREAM_FILE_SEEK = 3 };

static inline int64_t dream_file_operation(int32_t kind, int32_t fd, const char *path,
                                           const char *mode, uint8_t *bytes, int32_t count,
                                           int64_t position) {
    switch (kind) {
    case DREAM_FILE_OPEN: return dream_file_open(path, mode);
    case DREAM_FILE_READ: return dream_file_read(fd, bytes, count);
    case DREAM_FILE_WRITE: return dream_file_write(fd, bytes, count);
    case DREAM_FILE_SEEK: return dream_file_seek(fd, position);
    default: return -1;
    }
}
#endif
