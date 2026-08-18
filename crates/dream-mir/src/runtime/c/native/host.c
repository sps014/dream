#include "include/dream_rt_native.h"

#include <math.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#ifdef __APPLE__
#include <CommonCrypto/CommonDigest.h>
#include <CommonCrypto/CommonHMAC.h>
#endif

#ifdef _WIN32
#include <direct.h>
#include <io.h>
#else
#include <dirent.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>
#endif

__attribute__((constructor))
static void dream_stdio_linebuf(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
}

static char *dream_str_utf8(dream_ptr s) {
    int32_t n;
    char *out;
    int32_t i;
    const uint16_t *u;
    if (!s) {
        out = (char *)malloc(1);
        if (out) {
            out[0] = 0;
        }
        return out;
    }
    n = dream_str_len(s);
    out = (char *)malloc((size_t)n + 1);
    if (!out) {
        return NULL;
    }
    u = (const uint16_t *)((const char *)dream_p(s) + STRING_UTF8_OFFSET);
    for (i = 0; i < n; i++) {
        out[i] = u[i] < 128 ? (char)u[i] : '?';
    }
    out[n] = 0;
    return out;
}

static dream_ptr dream_str_from_utf8(const char *s) {
    size_t n = s ? strlen(s) : 0;
    size_t i;
    dream_ptr p = dream_string_alloc((int32_t)n);
    uint16_t *u = (uint16_t *)((char *)dream_p(p) + STRING_UTF8_OFFSET);
    for (i = 0; i < n; i++) {
        u[i] = (uint16_t)(unsigned char)s[i];
    }
    return p;
}

void print_int(int32_t v) { printf("%d", v); }

void print_char(int32_t c) {
    if (c == 10) {
        fputc('\n', stdout);
    } else {
        fputc((int)c, stdout);
    }
    fflush(stdout);
}

void print_string(dream_ptr s) {
    char *u = dream_str_utf8(s);
    if (u) {
        fputs(u, stdout);
        free(u);
    }
    fflush(stdout);
}

void print_float(float v) { printf("%.8g", (double)v); }

void print_double(double v) { printf("%.16g", v); }

double dream_host_abs(double v) { return fabs(v); }

dream_ptr fileRead(dream_ptr path) {
    char *p = dream_str_utf8(path);
    FILE *f;
    long sz;
    char *buf;
    dream_ptr out;
    if (!p) {
        return 0;
    }
    f = fopen(p, "rb");
    free(p);
    if (!f) {
        return 0;
    }
    fseek(f, 0, SEEK_END);
    sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (sz < 0) {
        fclose(f);
        return 0;
    }
    buf = (char *)malloc((size_t)sz + 1);
    if (!buf) {
        fclose(f);
        return 0;
    }
    fread(buf, 1, (size_t)sz, f);
    buf[sz] = 0;
    fclose(f);
    out = dream_str_from_utf8(buf);
    free(buf);
    return out;
}

int64_t fileWrite(dream_ptr path, dream_ptr contents) {
    char *p = dream_str_utf8(path);
    char *c = dream_str_utf8(contents);
    FILE *f;
    int64_t written = -1;
    if (!p) {
        free(c);
        return -1;
    }
    f = fopen(p, "wb");
    free(p);
    if (f) {
        if (c) {
            written = (int64_t)fwrite(c, 1, strlen(c), f);
        } else {
            written = 0;
        }
        fclose(f);
    }
    free(c);
    return written;
}

int64_t fileAppend(dream_ptr path, dream_ptr contents) {
    char *p = dream_str_utf8(path);
    char *c = dream_str_utf8(contents);
    FILE *f;
    int64_t written = -1;
    if (!p) {
        free(c);
        return -1;
    }
    f = fopen(p, "ab");
    free(p);
    if (f) {
        if (c) {
            written = (int64_t)fwrite(c, 1, strlen(c), f);
        } else {
            written = 0;
        }
        fclose(f);
    }
    free(c);
    return written;
}

int32_t fileExists(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = 0;
    if (p) {
        FILE *f = fopen(p, "rb");
        if (f) {
            ok = 1;
            fclose(f);
        }
        free(p);
    }
    return ok;
}

int32_t processPlatform(void) { return 0; }

dream_ptr processEnvGet(dream_ptr name) {
    char *key = dream_str_utf8(name);
    const char *value;
    dream_ptr result = 0;
    if (!key) {
        return 0;
    }
    value = getenv(key);
    if (value) {
        size_t n = strlen(value);
        char *tagged = (char *)malloc(n + 2);
        if (tagged) {
            tagged[0] = '1';
            memcpy(tagged + 1, value, n + 1);
            result = dream_str_from_utf8(tagged);
            free(tagged);
        }
    }
    free(key);
    return result;
}

void processEnvSet(dream_ptr name, dream_ptr value) {
    char *key = dream_str_utf8(name);
    char *text = dream_str_utf8(value);
    if (key && text) {
        setenv(key, text, 1);
    }
    free(key);
    free(text);
}

dream_ptr processCwd(void) {
    char path[4096];
    if (!getcwd(path, sizeof(path))) {
        return 0;
    }
    return dream_str_from_utf8(path);
}

int32_t processSetCwd(dream_ptr path) {
    char *text = dream_str_utf8(path);
    int32_t ok = text && chdir(text) == 0;
    free(text);
    return ok;
}

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
    return lseek(fd, (off_t)position, SEEK_SET) < 0 ? -1 : 0;
}

void fileHandleClose(int32_t fd) {
    if (fd >= 0) {
        close(fd);
    }
}

dream_ptr processRun(dream_ptr command, dream_ptr joined_args, dream_ptr cwd) {
#ifndef _WIN32
    char *program = dream_str_utf8(command);
    char *args = dream_str_utf8(joined_args);
    char *directory = dream_str_utf8(cwd);
    char *cursor;
    char *argv[128];
    char *output;
    size_t capacity = 4096;
    size_t used = 0;
    int pipe_fd[2];
    int status;
    pid_t pid;
    int argc = 0;
    dream_ptr result;
    if (!program || pipe(pipe_fd) != 0) {
        free(program);
        free(args);
        free(directory);
        return dream_array_new(0, 4);
    }
    argv[argc++] = program;
    cursor = args;
    while (cursor && *cursor && argc < 127) {
        char *next = strchr(cursor, '\n');
        if (next) {
            *next++ = 0;
        }
        argv[argc++] = cursor;
        cursor = next;
    }
    argv[argc] = NULL;
    pid = fork();
    if (pid == 0) {
        close(pipe_fd[0]);
        dup2(pipe_fd[1], STDOUT_FILENO);
        dup2(pipe_fd[1], STDERR_FILENO);
        close(pipe_fd[1]);
        if (directory && *directory) {
            chdir(directory);
        }
        execvp(program, argv);
        _exit(127);
    }
    free(args);
    free(directory);
    if (pid < 0) {
        close(pipe_fd[0]);
        close(pipe_fd[1]);
        free(program);
        return dream_array_new(0, 4);
    }
    close(pipe_fd[1]);
    output = (char *)malloc(capacity);
    if (!output) {
        close(pipe_fd[0]);
        waitpid(pid, &status, 0);
        free(program);
        return dream_array_new(0, 4);
    }
    for (;;) {
        ssize_t n;
        if (used == capacity) {
            char *grown = (char *)realloc(output, capacity * 2);
            if (!grown) {
                break;
            }
            output = grown;
            capacity *= 2;
        }
        n = read(pipe_fd[0], output + used, capacity - used);
        if (n <= 0) {
            break;
        }
        used += (size_t)n;
    }
    close(pipe_fd[0]);
    waitpid(pid, &status, 0);
    {
        int exit_code = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
        char header[64];
        int header_len = snprintf(header, sizeof(header), "%d\n%zu\n", exit_code, used);
        int32_t total = header_len + (int32_t)used;
        int32_t i;
        result = dream_array_new(total, 4);
        for (i = 0; i < header_len; i++) {
            ((int32_t *)((char *)dream_p(result) + 4))[i] = (unsigned char)header[i];
        }
        for (i = 0; i < (int32_t)used; i++) {
            ((int32_t *)((char *)dream_p(result) + 4))[header_len + i] =
                (unsigned char)output[i];
        }
    }
    free(output);
    free(program);
    return result;
#else
    (void)command;
    (void)joined_args;
    (void)cwd;
    return dream_array_new(0, 4);
#endif
}

static const uint8_t *dream_array_bytes(dream_ptr bytes, int32_t *len) {
    if (!bytes) {
        *len = 0;
        return NULL;
    }
    *len = dream_i32(bytes)[0];
    return (const uint8_t *)dream_p(bytes) + 4;
}

static dream_ptr dream_bytes(const uint8_t *data, int32_t len) {
    dream_ptr bytes = dream_array_new(len, 1);
    if (len > 0) {
        memcpy((char *)dream_p(bytes) + 4, data, (size_t)len);
    }
    return bytes;
}

dream_ptr cryptoSha256(dream_ptr input) {
#ifdef __APPLE__
    uint8_t hash[CC_SHA256_DIGEST_LENGTH];
    int32_t len;
    const uint8_t *data = dream_array_bytes(input, &len);
    CC_SHA256(data, (CC_LONG)len, hash);
    return dream_bytes(hash, CC_SHA256_DIGEST_LENGTH);
#else
    (void)input;
    return dream_array_new(0, 1);
#endif
}

dream_ptr cryptoSha512(dream_ptr input) {
#ifdef __APPLE__
    uint8_t hash[CC_SHA512_DIGEST_LENGTH];
    int32_t len;
    const uint8_t *data = dream_array_bytes(input, &len);
    CC_SHA512(data, (CC_LONG)len, hash);
    return dream_bytes(hash, CC_SHA512_DIGEST_LENGTH);
#else
    (void)input;
    return dream_array_new(0, 1);
#endif
}

dream_ptr cryptoHmacSha256(dream_ptr key, dream_ptr input) {
#ifdef __APPLE__
    uint8_t hash[CC_SHA256_DIGEST_LENGTH];
    int32_t key_len;
    int32_t input_len;
    const uint8_t *key_data = dream_array_bytes(key, &key_len);
    const uint8_t *input_data = dream_array_bytes(input, &input_len);
    CCHmac(kCCHmacAlgSHA256, key_data, (size_t)key_len, input_data, (size_t)input_len, hash);
    return dream_bytes(hash, CC_SHA256_DIGEST_LENGTH);
#else
    (void)key;
    (void)input;
    return dream_array_new(0, 1);
#endif
}

dream_ptr cryptoSecureRandomBytes(int32_t len) {
    dream_ptr bytes;
    if (len < 0) {
        len = 0;
    }
    bytes = dream_array_new(len, 1);
#ifdef __APPLE__
    arc4random_buf((char *)dream_p(bytes) + 4, (size_t)len);
#endif
    return bytes;
}

void cryptoSecureRandomFill(dream_ptr bytes) {
    int32_t len;
    (void)dream_array_bytes(bytes, &len);
#ifdef __APPLE__
    if (bytes && len > 0) {
        arc4random_buf((char *)dream_p(bytes) + 4, (size_t)len);
    }
#endif
}

int64_t dateNowMillis(void) {
    return (int64_t)time(NULL) * 1000;
}

int64_t timeNowNanos(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + (int64_t)ts.tv_nsec;
}

int64_t Time_nano_time(void) { return timeNowNanos(); }

int32_t dateLocalOffsetMinutes(void) { return 0; }

void consoleExit(int32_t code) { exit(code); }

