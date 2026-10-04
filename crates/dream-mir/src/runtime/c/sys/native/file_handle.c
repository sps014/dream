#include "dream_host_support.h"

#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>

#ifdef _WIN32
#include <io.h>
typedef intptr_t ssize_t;
/* MSVC's off_t/lseek are 32-bit; use the 64-bit variants for file handles. */
typedef __int64 dream_off_t;
#define dream_lseek _lseeki64
#else
#include <unistd.h>
typedef off_t dream_off_t;
#define dream_lseek lseek
#endif

int32_t fileOpen(dream_ptr path, dream_ptr mode) {
    char *text = dream_str_utf8(path);
    char *open_mode = dream_str_utf8(mode);
    FILE *file;
    int32_t fd;
    if (!text || !open_mode || !*open_mode) {
        free(text);
        free(open_mode);
        return -3;
    }
    file = fopen(text, open_mode);
    free(text);
    free(open_mode);
    if (!file) {
        return errno == ENOENT ? -1 : errno == EACCES ? -2 : -3;
    }
    fd = dup(fileno(file));
    fclose(file);
    return fd < 0 ? -3 : fd;
}

dream_ptr fileHandleRead(int32_t fd, int32_t count) {
    dream_ptr bytes;
    ssize_t n;
    if (count < 0) {
        count = 0;
    }
    bytes = dream_array_new(count, 1);
    n = read(fd, (char *)dream_p(bytes) + 4, (size_t)count);
    if (n < 0) {
        dream_i32(bytes)[0] = 0;
    } else {
        dream_i32(bytes)[0] = (int32_t)n;
    }
    return bytes;
}

int64_t fileHandleWrite(int32_t fd, dream_ptr data) {
    int32_t n = data ? dream_i32(data)[0] : 0;
    ssize_t written = write(fd, data ? (char *)dream_p(data) + 4 : "", (size_t)n);
    return written < 0 ? -1 : (int64_t)written;
}

int32_t fileHandleSeek(int32_t fd, int64_t position) {
    return dream_lseek(fd, (dream_off_t)position, SEEK_SET) < 0 ? -1 : 0;
}

int64_t fileHandleTell(int32_t fd) {
    dream_off_t pos = dream_lseek(fd, 0, SEEK_CUR);
    return pos < 0 ? -1 : (int64_t)pos;
}

int32_t fileHandleSeekEnd(int32_t fd, int64_t offset) {
    return dream_lseek(fd, (dream_off_t)offset, SEEK_END) < 0 ? -1 : 0;
}

void fileHandleClose(int32_t fd) {
    if (fd >= 0) {
        close(fd);
    }
}
