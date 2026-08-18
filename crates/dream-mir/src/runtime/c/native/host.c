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
#include <signal.h>
#endif

__attribute__((constructor))
static void dream_stdio_linebuf(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
}

static char *dream_str_utf8(dream_ptr s) {
    int32_t n;
    char *out;
    int32_t i;
    size_t used = 0;
    const uint16_t *u;
    if (!s) {
        out = (char *)malloc(1);
        if (out) {
            out[0] = 0;
        }
        return out;
    }
    n = dream_str_len(s);
    out = (char *)malloc((size_t)n * 3 + 1);
    if (!out) {
        return NULL;
    }
    u = dream_str_units(s);
    for (i = 0; i < n; i++) {
        uint32_t cp = u[i];
        if (cp >= 0xD800 && cp <= 0xDBFF && i + 1 < n) {
            uint16_t low = u[i + 1];
            if (low >= 0xDC00 && low <= 0xDFFF) {
                cp = 0x10000 + (((cp - 0xD800) << 10) | (low - 0xDC00));
                i += 1;
            }
        }
        if (cp < 0x80) {
            out[used++] = (char)cp;
        } else if (cp < 0x800) {
            out[used++] = (char)(0xC0 | (cp >> 6));
            out[used++] = (char)(0x80 | (cp & 0x3F));
        } else if (cp < 0x10000) {
            out[used++] = (char)(0xE0 | (cp >> 12));
            out[used++] = (char)(0x80 | ((cp >> 6) & 0x3F));
            out[used++] = (char)(0x80 | (cp & 0x3F));
        } else {
            out[used++] = (char)(0xF0 | (cp >> 18));
            out[used++] = (char)(0x80 | ((cp >> 12) & 0x3F));
            out[used++] = (char)(0x80 | ((cp >> 6) & 0x3F));
            out[used++] = (char)(0x80 | (cp & 0x3F));
        }
    }
    out[used] = 0;
    return out;
}

static dream_ptr dream_str_from_utf8(const char *s) {
    size_t n = s ? strlen(s) : 0;
    size_t i = 0;
    size_t units = 0;
    const unsigned char *in = (const unsigned char *)s;
    uint16_t *u;
    dream_ptr p;
    while (i < n) {
        unsigned char c = in[i];
        uint32_t cp;
        if (c < 0x80) {
            cp = c;
            i += 1;
        } else if ((c & 0xE0) == 0xC0 && i + 1 < n) {
            cp = ((uint32_t)(c & 0x1F) << 6) | (uint32_t)(in[i + 1] & 0x3F);
            i += 2;
        } else if ((c & 0xF0) == 0xE0 && i + 2 < n) {
            cp = ((uint32_t)(c & 0x0F) << 12) | ((uint32_t)(in[i + 1] & 0x3F) << 6)
                | (uint32_t)(in[i + 2] & 0x3F);
            i += 3;
        } else if ((c & 0xF8) == 0xF0 && i + 3 < n) {
            cp = ((uint32_t)(c & 0x07) << 18) | ((uint32_t)(in[i + 1] & 0x3F) << 12)
                | ((uint32_t)(in[i + 2] & 0x3F) << 6) | (uint32_t)(in[i + 3] & 0x3F);
            i += 4;
        } else {
            cp = 0xFFFD;
            i += 1;
        }
        units += (cp > 0xFFFF) ? 2 : 1;
    }
    p = dream_string_alloc((int32_t)units);
    u = (uint16_t *)((char *)dream_p(p) + STRING_UNITS_OFFSET);
    i = 0;
    units = 0;
    while (i < n) {
        unsigned char c = in[i];
        uint32_t cp;
        if (c < 0x80) {
            cp = c;
            i += 1;
        } else if ((c & 0xE0) == 0xC0 && i + 1 < n) {
            cp = ((uint32_t)(c & 0x1F) << 6) | (uint32_t)(in[i + 1] & 0x3F);
            i += 2;
        } else if ((c & 0xF0) == 0xE0 && i + 2 < n) {
            cp = ((uint32_t)(c & 0x0F) << 12) | ((uint32_t)(in[i + 1] & 0x3F) << 6)
                | (uint32_t)(in[i + 2] & 0x3F);
            i += 3;
        } else if ((c & 0xF8) == 0xF0 && i + 3 < n) {
            cp = ((uint32_t)(c & 0x07) << 18) | ((uint32_t)(in[i + 1] & 0x3F) << 12)
                | ((uint32_t)(in[i + 2] & 0x3F) << 6) | (uint32_t)(in[i + 3] & 0x3F);
            i += 4;
        } else {
            cp = 0xFFFD;
            i += 1;
        }
        if (cp > 0xFFFF) {
            cp -= 0x10000;
            u[units++] = (uint16_t)(0xD800 + (cp >> 10));
            u[units++] = (uint16_t)(0xDC00 + (cp & 0x3FF));
        } else {
            u[units++] = (uint16_t)cp;
        }
    }
    return p;
}

static dream_ptr dream_chars_from_utf8(const char *s) {
    size_t n = s ? strlen(s) : 0;
    dream_ptr p = dream_array_new((int32_t)n, 1);
    if (n) {
        memcpy((char *)dream_p(p) + 4, s, n);
    }
    *(int32_t *)((char *)dream_p(p) - 4) = INT32_MAX;
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

static void print_float_shortest(float v) {
    char text[32];
    char *end;
    snprintf(text, sizeof(text), "%.6f", (double)v);
    end = text + strlen(text);
    while (end > text && end[-1] == '0') {
        *--end = 0;
    }
    if (end > text && end[-1] == '.') {
        *--end = 0;
    }
    fputs(text, stdout);
}

void print_float(float v) { print_float_shortest(v); }

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

int32_t fileDelete(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = 0;
    if (p) {
        ok = remove(p) == 0;
        free(p);
    }
    return ok;
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

int64_t fileSize(dream_ptr path) {
#ifndef _WIN32
    char *text = dream_str_utf8(path);
    struct stat st;
    int64_t size = -1;
    if (text && stat(text, &st) == 0) {
        size = (int64_t)st.st_size;
    }
    free(text);
    return size;
#else
    (void)path;
    return -1;
#endif
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
        int exit_code = WIFEXITED(status) ? WEXITSTATUS(status) : -2;
        char header[64];
        int header_len;
        if (exit_code == 127 && used == 0) {
            const char *message = "failed to spawn process";
            header_len = snprintf(header, sizeof(header), "-1\n");
            used = strlen(message);
            memcpy(output, message, used);
        } else {
            header_len = snprintf(header, sizeof(header), "%d\n%zu\n", exit_code, used);
        }
        int32_t total = header_len + (int32_t)used;
        result = dream_array_new(total, 1);
        memcpy((char *)dream_p(result) + 4, header, (size_t)header_len);
        if (used) {
            memcpy((char *)dream_p(result) + 4 + header_len, output, used);
        }
        *(int32_t *)((char *)dream_p(result) - 4) = INT32_MAX;
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

#ifndef _WIN32
#define DREAM_CHILDREN_MAX 32
typedef struct {
    pid_t pid;
    int stdin_fd;
    int stdout_fd;
    int stderr_fd;
} DreamChild;

static DreamChild dream_children[DREAM_CHILDREN_MAX];

static DreamChild *dream_child(int32_t handle) {
    if (handle <= 0 || handle > DREAM_CHILDREN_MAX) {
        return NULL;
    }
    return dream_children[handle - 1].pid ? &dream_children[handle - 1] : NULL;
}

static void dream_child_close(DreamChild *child) {
    if (child->stdin_fd >= 0) close(child->stdin_fd);
    if (child->stdout_fd >= 0) close(child->stdout_fd);
    if (child->stderr_fd >= 0) close(child->stderr_fd);
    memset(child, 0, sizeof(*child));
}

dream_ptr processSpawn(dream_ptr command, dream_ptr joined_args, dream_ptr cwd) {
    char *program = dream_str_utf8(command);
    char *args = dream_str_utf8(joined_args);
    char *directory = dream_str_utf8(cwd);
    char *argv[128];
    char *cursor;
    int argc = 0;
    int in_pipe[2], out_pipe[2], err_pipe[2];
    int slot;
    pid_t pid;
    if (!program || pipe(in_pipe) || pipe(out_pipe) || pipe(err_pipe)) {
        free(program); free(args); free(directory);
        return dream_chars_from_utf8("-1\nfailed to spawn process");
    }
    argv[argc++] = program;
    cursor = args;
    while (cursor && *cursor && argc < 127) {
        char *next = strchr(cursor, '\n');
        if (next) *next++ = 0;
        argv[argc++] = cursor;
        cursor = next;
    }
    argv[argc] = NULL;
    for (slot = 0; slot < DREAM_CHILDREN_MAX && dream_children[slot].pid; slot++) {}
    if (slot == DREAM_CHILDREN_MAX || (pid = fork()) < 0) {
        close(in_pipe[0]); close(in_pipe[1]); close(out_pipe[0]); close(out_pipe[1]);
        close(err_pipe[0]); close(err_pipe[1]);
        free(program); free(args); free(directory);
        return dream_chars_from_utf8("-1\nfailed to spawn process");
    }
    if (pid == 0) {
        dup2(in_pipe[0], STDIN_FILENO);
        dup2(out_pipe[1], STDOUT_FILENO);
        dup2(err_pipe[1], STDERR_FILENO);
        close(in_pipe[0]); close(in_pipe[1]); close(out_pipe[0]); close(out_pipe[1]);
        close(err_pipe[0]); close(err_pipe[1]);
        if (directory && *directory) chdir(directory);
        execvp(program, argv);
        _exit(127);
    }
    close(in_pipe[0]); close(out_pipe[1]); close(err_pipe[1]);
    dream_children[slot].pid = pid;
    dream_children[slot].stdin_fd = in_pipe[1];
    dream_children[slot].stdout_fd = out_pipe[0];
    dream_children[slot].stderr_fd = err_pipe[0];
    free(program); free(args); free(directory);
    {
        char wire[32];
        snprintf(wire, sizeof(wire), "%d\n", slot + 1);
        return dream_chars_from_utf8(wire);
    }
}

int32_t processWriteStdin(int32_t handle, dream_ptr data) {
    DreamChild *child = dream_child(handle);
    int32_t len = data ? dream_i32(data)[0] : 0;
    if (!child || child->stdin_fd < 0) return 0;
    return write(child->stdin_fd, (char *)dream_p(data) + 4, (size_t)len) == len;
}

static dream_ptr dream_child_read(int32_t handle, int32_t stream, int32_t max_bytes, int line) {
    DreamChild *child = dream_child(handle);
    int fd;
    char buf[4097];
    ssize_t n;
    if (!child) return dream_chars_from_utf8(line ? "0" : "");
    fd = stream ? child->stderr_fd : child->stdout_fd;
    if (fd < 0) return dream_chars_from_utf8(line ? "0" : "");
    if (line) {
        size_t used = 1;
        buf[0] = '1';
        while (used < sizeof(buf) - 1 && (n = read(fd, buf + used, 1)) == 1) {
            if (buf[used++] == '\n') { used--; break; }
        }
        if (used == 1) return dream_chars_from_utf8("0");
        buf[used] = 0;
        return dream_chars_from_utf8(buf);
    }
    if (max_bytes < 0) max_bytes = 0;
    if (max_bytes > 4096) max_bytes = 4096;
    n = read(fd, buf, (size_t)max_bytes);
    if (n <= 0) return dream_chars_from_utf8("");
    buf[n] = 0;
    return dream_chars_from_utf8(buf);
}

dream_ptr processReadStream(int32_t handle, int32_t stream, int32_t max_bytes) {
    return dream_child_read(handle, stream, max_bytes, 0);
}

dream_ptr processReadStreamLine(int32_t handle, int32_t stream) {
    return dream_child_read(handle, stream, 4096, 1);
}

dream_ptr processWait(int32_t handle) {
    DreamChild *child = dream_child(handle);
    int status;
    char wire[32];
    if (!child || waitpid(child->pid, &status, 0) < 0) return dream_chars_from_utf8("-1");
    snprintf(wire, sizeof(wire), "%d", WIFEXITED(status) ? WEXITSTATUS(status) : -2);
    dream_child_close(child);
    return dream_chars_from_utf8(wire);
}

int32_t processKill(int32_t handle) {
    DreamChild *child = dream_child(handle);
    return child && kill(child->pid, SIGKILL) == 0;
}
#endif

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

static int32_t dream_zone_offset_minutes(const char *zone, int64_t epoch_millis) {
#ifndef _WIN32
    const char *previous = getenv("TZ");
    char *saved = previous ? strdup(previous) : NULL;
    struct stat zoneinfo;
    time_t instant = (time_t)(epoch_millis / 1000);
    struct tm local_tm;
    struct tm utc_tm;
    int32_t offset;
    if (!zone || !*zone) return -999999;
    if (strcmp(zone, "UTC") != 0) {
        char path[4096];
        if (snprintf(path, sizeof(path), "/usr/share/zoneinfo/%s", zone) >= (int)sizeof(path)
            || stat(path, &zoneinfo) != 0) {
            return -999999;
        }
    }
    setenv("TZ", zone, 1);
    tzset();
    if (!localtime_r(&instant, &local_tm) || !gmtime_r(&instant, &utc_tm)) {
        offset = -999999;
    } else {
        utc_tm.tm_isdst = -1;
        offset = (int32_t)(difftime(mktime(&local_tm), mktime(&utc_tm)) / 60);
    }
    if (saved) {
        setenv("TZ", saved, 1);
    } else {
        unsetenv("TZ");
    }
    free(saved);
    tzset();
    return offset;
#else
    (void)zone;
    (void)epoch_millis;
    return -999999;
#endif
}

int32_t dateLocalOffsetMinutes(int64_t epoch_millis) {
#ifndef _WIN32
    time_t instant = (time_t)(epoch_millis / 1000);
    struct tm local_tm;
    struct tm utc_tm;
    if (!localtime_r(&instant, &local_tm) || !gmtime_r(&instant, &utc_tm)) return 0;
    utc_tm.tm_isdst = -1;
    return (int32_t)(difftime(mktime(&local_tm), mktime(&utc_tm)) / 60);
#else
    (void)epoch_millis;
    return 0;
#endif
}

int32_t dateZoneOffsetMinutes(dream_ptr zone_name, int64_t epoch_millis) {
    char *zone = dream_str_utf8(zone_name);
    int32_t offset = dream_zone_offset_minutes(zone, epoch_millis);
    free(zone);
    return offset;
}

dream_ptr dateLocalZoneName(void) {
#ifndef _WIN32
    tzset();
    return dream_str_from_utf8(tzname[0] ? tzname[0] : "UTC");
#else
    return dream_str_from_utf8("UTC");
#endif
}

dream_ptr httpRequestStream(
    dream_ptr url, dream_ptr method, dream_ptr headers, dream_ptr body, int32_t timeout_ms,
    int32_t http_version) {
    (void)url; (void)method; (void)headers; (void)body; (void)timeout_ms; (void)http_version;
    return dream_chars_from_utf8("-1\nnative C HTTP streams are unsupported");
}

dream_ptr tcpConnect(dream_ptr host, int32_t port, int32_t timeout_ms) {
    (void)host; (void)port; (void)timeout_ms;
    return dream_chars_from_utf8("-1\nnative C TCP connections are unsupported");
}

dream_ptr wsConnect(dream_ptr url, int32_t timeout_ms) {
    char *text = dream_str_utf8(url);
    dream_ptr result;
    (void)timeout_ms;
    if (text && strncmp(text, "ws://", 5) != 0 && strncmp(text, "wss://", 6) != 0) {
        result = dream_chars_from_utf8("-2\nunsupported WebSocket scheme");
    } else {
        result = dream_chars_from_utf8("-1\nnative C WebSocket connections are unsupported");
    }
    free(text);
    return result;
}

int32_t tcpClose(int32_t handle) {
    (void)handle;
    return 0;
}

int32_t wsClose(int32_t handle, int32_t code, dream_ptr reason) {
    (void)handle;
    (void)code;
    (void)reason;
    return 0;
}

void consoleExit(int32_t code) { exit(code); }

