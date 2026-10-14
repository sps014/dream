#ifndef DREAM_FILE_H
#define DREAM_FILE_H
#include <stdint.h>

int32_t dream_file_open(const char *path, const char *mode);
int32_t dream_file_duplicate(int32_t fd);
int64_t dream_file_read(int32_t fd, void *buffer, int32_t count);
int64_t dream_file_write(int32_t fd, const void *buffer, int32_t count);
int32_t dream_file_seek(int32_t fd, int64_t position);
int32_t dream_file_close(int32_t fd);
#endif
