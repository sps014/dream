#include "dream_host_support.h"
#include "dream_file.h"

#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <stdio.h>
#include <sys/stat.h>

#ifdef _WIN32
#include <io.h>
typedef intptr_t ssize_t;
/* MSVC's off_t/lseek are 32-bit; use the 64-bit variants for file handles. */
typedef __int64 dream_off_t;
#define dream_lseek _lseeki64
/* The UCRT exports only the ISO-conforming names; the POSIX spellings live in oldnames.lib. */
#define dream_dup _dup
#define dream_fileno _fileno
#define dream_read(fd, buf, n) _read(fd, buf, (unsigned)(n))
#define dream_write(fd, buf, n) _write(fd, buf, (unsigned)(n))
#define dream_close _close
#else
#include <unistd.h>
typedef off_t dream_off_t;
#define dream_lseek lseek
#define dream_dup dup
#define dream_fileno fileno
#define dream_read read
#define dream_write write
#define dream_close close
#endif

int32_t dream_file_open(const char *text, const char *open_mode) {
    if (!text || !open_mode || !*open_mode) { return -3; }
    if (strcmp(open_mode, "r") && strcmp(open_mode, "w") && strcmp(open_mode, "a") &&
        strcmp(open_mode, "r+") && strcmp(open_mode, "w+") && strcmp(open_mode, "a+")) {
        return -3;
    }
    char binary_mode[4] = {open_mode[0], 'b', 0, 0};
    if (open_mode[1] == '+') { binary_mode[2] = '+'; }
    FILE *file = fopen(text, binary_mode);
    if (!file) { return errno == ENOENT ? -1 : errno == EACCES ? -2 : -4; }
    int32_t fd = dream_dup(dream_fileno(file));
    fclose(file);
    return fd < 0 ? -4 : fd;
}

int32_t fileOpen(dream_ptr path, dream_ptr mode) {
    char *text = dream_str_utf8(path);
    char *open_mode = dream_str_utf8(mode);
    int32_t fd = dream_file_open(text, open_mode);
    free(text);
    free(open_mode);
    return fd;
}

int32_t dream_file_duplicate(int32_t fd) { return dream_dup(fd); }

int64_t dream_file_read(int32_t fd, void *buffer, int32_t count) {
    ssize_t n;
    do { n = dream_read(fd, buffer, (size_t)count); } while (n < 0 && errno == EINTR);
    return n < 0 ? -1 : (int64_t)n;
}

int64_t dream_file_write(int32_t fd, const void *buffer, int32_t count) {
    ssize_t n;
    do { n = dream_write(fd, buffer, (size_t)count); } while (n < 0 && errno == EINTR);
    return n < 0 ? -1 : (int64_t)n;
}

int32_t dream_file_seek(int32_t fd, int64_t position) {
    return dream_lseek(fd, (dream_off_t)position, SEEK_SET) < 0 ? -1 : 0;
}

int32_t dream_file_close(int32_t fd) {
    /* Never retry close: an interrupted close may already have released the descriptor. */
    return fd < 0 ? -1 : dream_close(fd);
}

/* A leading status byte keeps read errors distinct from successful EOF across the JS ABI. */
dream_ptr fileHandleRead(int32_t fd, int32_t count) {
    if (count < 0 || count == INT32_MAX) {
        dream_ptr error = dream_array_new(1, 1);
        ((uint8_t *)dream_p(error))[4] = 1;
        return error;
    }
    dream_ptr bytes = dream_array_new(count + 1, 1);
    uint8_t *payload = (uint8_t *)dream_p(bytes) + 4;
    int64_t n = dream_file_read(fd, payload + 1, count);
    payload[0] = n < 0 ? 1 : 0;
    dream_i32(bytes)[0] = n < 0 ? 1 : (int32_t)n + 1;
    return bytes;
}

int64_t fileHandleWrite(int32_t fd, dream_ptr data) {
    int32_t n = data ? dream_i32(data)[0] : 0;
    int64_t written = dream_file_write(fd, data ? (char *)dream_p(data) + 4 : "", n);
    return written < 0 ? -1 : (int64_t)written;
}

int32_t fileHandleSeek(int32_t fd, int64_t position) {
    return dream_file_seek(fd, position);
}

int64_t fileHandleTell(int32_t fd) {
    dream_off_t pos = dream_lseek(fd, 0, SEEK_CUR);
    return pos < 0 ? -1 : (int64_t)pos;
}

int32_t fileHandleSeekEnd(int32_t fd, int64_t offset) {
    return dream_lseek(fd, (dream_off_t)offset, SEEK_END) < 0 ? -1 : 0;
}

int64_t fileHandleLength(int32_t fd) {
#ifdef _WIN32
    struct _stat64 info;
    return _fstat64(fd, &info) < 0 ? -1 : (int64_t)info.st_size;
#else
    struct stat info;
    return fstat(fd, &info) < 0 ? -1 : (int64_t)info.st_size;
#endif
}

int32_t fileHandleClose(int32_t fd) {
    return dream_file_close(fd);
}
