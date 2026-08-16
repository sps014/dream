#include "dream_rt.h"

#if defined(__wasm__)
#define DREAM_FREESTANDING 1
#endif

#ifndef DREAM_FREESTANDING
#include <dirent.h>
#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#else
static double fabs(double x) { return x < 0 ? -x : x; }
static double floor(double x) { return __builtin_floor(x); }
static double ceil(double x) { return __builtin_ceil(x); }
static double round(double x) { return __builtin_round(x); }
static double sqrt(double x) { return __builtin_sqrt(x); }
static double pow(double a, double b) { return __builtin_pow(a, b); }
static double sin(double x) { return __builtin_sin(x); }
static double cos(double x) { return __builtin_cos(x); }
static double tan(double x) { return __builtin_tan(x); }
static double asin(double x) { return __builtin_asin(x); }
static double acos(double x) { return __builtin_acos(x); }
static double atan(double x) { return __builtin_atan(x); }
static double atan2(double y, double x) { return __builtin_atan2(y, x); }
#endif

#ifndef DREAM_FREESTANDING
#ifdef __APPLE__
#include <mach-o/dyld.h>
#endif
#endif

static int g_argc;
static char **g_argv;

#define UNKNOWN_ZONE_OFFSET (-999999)

void dream_rt_set_args(int argc, char **argv) {
    g_argc = argc;
    g_argv = argv;
}


static void guest_path(int32_t ptr, char *out, size_t cap) {
    int32_t n = dream_str_byte_size(ptr);
    uint8_t *h = dream_heap_base();
    if (n < 0) {
        n = 0;
    }
    if ((size_t)n >= cap) {
        n = (int32_t)cap - 1;
    }
    if (n > 0 && h) {
        memcpy(out, h + (uint32_t)ptr + DREAM_STRING_UTF8_OFFSET, (size_t)n);
    }
    out[n] = 0;
}

double dream_math_abs(double x) { return fabs(x); }
double dream_math_floor(double x) { return floor(x); }
double dream_math_ceil(double x) { return ceil(x); }
double dream_math_round(double x) { return round(x); }
double dream_math_sqrt(double x) { return sqrt(x); }
double dream_math_pow(double a, double b) { return pow(a, b); }
double dream_math_sin(double x) { return sin(x); }
double dream_math_cos(double x) { return cos(x); }
double dream_math_tan(double x) { return tan(x); }
double dream_math_asin(double x) { return asin(x); }
double dream_math_acos(double x) { return acos(x); }
double dream_math_atan(double x) { return atan(x); }
double dream_math_atan2(double y, double x) { return atan2(y, x); }

#ifdef DREAM_FREESTANDING
int32_t dream_date_local_offset_minutes(int64_t millis) {
    (void)millis;
    return 0;
}

int32_t dream_date_zone_offset_minutes(int32_t zone_ptr, int64_t millis) {
    (void)zone_ptr;
    (void)millis;
    return UNKNOWN_ZONE_OFFSET;
}

int32_t dream_date_local_zone_name(void) {
    return dream_intern_utf8("", 0);
}
#endif

int32_t dream_file_read(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    FILE *f = fopen(p, "rb");
    if (!f) {
        return dream_intern_utf8("", 0);
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return dream_intern_utf8("", 0);
    }
    long n = ftell(f);
    if (n < 0) {
        fclose(f);
        return dream_intern_utf8("", 0);
    }
    rewind(f);
    char *buf = (char *)malloc((size_t)n);
    if (!buf) {
        fclose(f);
        return dream_intern_utf8("", 0);
    }
    size_t got = fread(buf, 1, (size_t)n, f);
    fclose(f);
    int32_t s = dream_intern_utf8(buf, (int32_t)got);
    free(buf);
    return s;
#else
    (void)path;
    return dream_intern_utf8("", 0);
#endif
}

static int64_t write_file(int32_t path, int32_t content, const char *mode) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    FILE *f = fopen(p, mode);
    if (!f) {
        return -1;
    }
    int32_t n = dream_str_byte_size(content);
    uint8_t *h = dream_heap_base();
    size_t w = 0;
    if (n > 0 && h) {
        w = fwrite(h + (uint32_t)content + DREAM_STRING_UTF8_OFFSET, 1, (size_t)n, f);
    }
    fclose(f);
    return w == (size_t)n ? (int64_t)w : -1;
#else
    (void)path;
    (void)content;
    (void)mode;
    return -1;
#endif
}

int64_t dream_file_write(int32_t path, int32_t content) { return write_file(path, content, "wb"); }
int64_t dream_file_append(int32_t path, int32_t content) { return write_file(path, content, "ab"); }

int32_t dream_file_read_bytes(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    FILE *f = fopen(p, "rb");
    if (!f) {
        int32_t empty = dream_malloc(4, DREAM_TAG_ARRAY);
        dream_store_i32(empty, 0);
        return empty;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        int32_t empty = dream_malloc(4, DREAM_TAG_ARRAY);
        dream_store_i32(empty, 0);
        return empty;
    }
    long n = ftell(f);
    if (n < 0) {
        n = 0;
    }
    rewind(f);
    int32_t arr = dream_malloc(4 + (int32_t)n, DREAM_TAG_ARRAY);
    dream_store_i32(arr, (int32_t)n);
    uint8_t *h = dream_heap_base();
    if (n > 0 && h) {
        size_t got = fread(h + (uint32_t)arr + 4, 1, (size_t)n, f);
        dream_store_i32(arr, (int32_t)got);
    }
    fclose(f);
    return arr;
#else
    (void)path;
    int32_t empty = dream_malloc(4, DREAM_TAG_ARRAY);
    dream_store_i32(empty, 0);
    return empty;
#endif
}

int64_t dream_file_write_bytes(int32_t path, int32_t data) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    FILE *f = fopen(p, "wb");
    if (!f) {
        return -1;
    }
    int32_t n = data > 0 ? dream_load_i32(data) : 0;
    uint8_t *h = dream_heap_base();
    size_t w = 0;
    if (n > 0 && h) {
        w = fwrite(h + (uint32_t)data + 4, 1, (size_t)n, f);
    }
    fclose(f);
    return w == (size_t)n ? (int64_t)w : -1;
#else
    (void)path;
    (void)data;
    return -1;
#endif
}


int32_t dream_file_exists(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    return access(p, F_OK) == 0 ? 1 : 0;
#else
    (void)path;
    return 0;
#endif
}

int32_t dream_file_delete(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    return remove(p) == 0 ? 1 : 0;
#else
    (void)path;
    return 0;
#endif
}

int64_t dream_file_size(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    struct stat st;
    if (stat(p, &st) != 0) {
        return -1;
    }
    return (int64_t)st.st_size;
#else
    (void)path;
    return -1;
#endif
}

int32_t dream_file_is_dir(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    struct stat st;
    if (stat(p, &st) != 0) {
        return 0;
    }
    return S_ISDIR(st.st_mode) ? 1 : 0;
#else
    (void)path;
    return 0;
#endif
}

int32_t dream_dir_list(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    DIR *d = opendir(p);
    if (!d) {
        return dream_intern_utf8("", 0);
    }
    size_t cap = 256;
    size_t n = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) {
        closedir(d);
        return dream_intern_utf8("", 0);
    }
    buf[0] = 0;
    struct dirent *ent;
    while ((ent = readdir(d))) {
        if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) {
            continue;
        }
        size_t ln = strlen(ent->d_name);
        size_t add = ln + (n ? 1 : 0);
        if (n + add + 1 > cap) {
            cap = (n + add + 1) * 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) {
                break;
            }
            buf = nb;
        }
        if (n) {
            buf[n++] = '\n';
        }
        memcpy(buf + n, ent->d_name, ln);
        n += ln;
        buf[n] = 0;
    }
    closedir(d);
    int32_t s = dream_intern_utf8(buf, (int32_t)n);
    free(buf);
    return s;
#else
    (void)path;
    return dream_intern_utf8("", 0);
#endif
}

int32_t dream_dir_create(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    if (mkdir(p, 0755) == 0 || errno == EEXIST) {
        return 1;
    }
    return 0;
#else
    (void)path;
    return 0;
#endif
}

int32_t dream_dir_create_all(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    for (char *s = p + 1; *s; s++) {
        if (*s == '/') {
            *s = 0;
            mkdir(p, 0755);
            *s = '/';
        }
    }
    if (mkdir(p, 0755) == 0 || errno == EEXIST) {
        return 1;
    }
    return 0;
#else
    (void)path;
    return 0;
#endif
}

int32_t dream_process_platform(void) { return 0; }
int32_t dream_process_os_family(void) {
#ifdef _WIN32
    return 1;
#else
    return 0;
#endif
}

int32_t dream_process_cwd(void) {
#ifndef DREAM_FREESTANDING
    char buf[4096];
    if (!getcwd(buf, sizeof(buf))) {
        return dream_intern_utf8("", 0);
    }
    return dream_intern_utf8(buf, (int32_t)strlen(buf));
#else
    return dream_intern_utf8("", 0);
#endif
}

int32_t dream_process_set_cwd(int32_t path) {
#ifndef DREAM_FREESTANDING
    char p[4096];
    guest_path(path, p, sizeof(p));
    return chdir(p) == 0 ? 1 : 0;
#else
    (void)path;
    return 0;
#endif
}

int32_t dream_process_args(void) {
#ifndef DREAM_FREESTANDING
    size_t cap = 256;
    size_t n = 0;
    char *buf = (char *)malloc(cap);
    if (!buf) {
        return dream_intern_utf8("", 0);
    }
    buf[0] = 0;
    for (int i = 1; i < g_argc; i++) {
        size_t ln = strlen(g_argv[i]);
        size_t add = ln + (n ? 1 : 0);
        if (n + add + 1 > cap) {
            cap = (n + add + 1) * 2;
            char *nb = (char *)realloc(buf, cap);
            if (!nb) {
                break;
            }
            buf = nb;
        }
        if (n) {
            buf[n++] = '\n';
        }
        memcpy(buf + n, g_argv[i], ln);
        n += ln;
        buf[n] = 0;
    }
    int32_t s = dream_intern_utf8(buf, (int32_t)n);
    free(buf);
    return s;
#else
    return dream_intern_utf8("", 0);
#endif
}

int32_t dream_process_exe_path(void) {
#ifndef DREAM_FREESTANDING
    char buf[4096];
#ifdef __APPLE__
    uint32_t sz = sizeof(buf);
    if (_NSGetExecutablePath(buf, &sz) != 0) {
        return dream_intern_utf8("", 0);
    }
    return dream_intern_utf8(buf, (int32_t)strlen(buf));
#else
    ssize_t n = readlink("/proc/self/exe", buf, sizeof(buf) - 1);
    if (n < 0) {
        return dream_intern_utf8("", 0);
    }
    buf[n] = 0;
    return dream_intern_utf8(buf, (int32_t)n);
#endif
#else
    return dream_intern_utf8("", 0);
#endif
}

int32_t dream_process_env_get(int32_t key) {
#ifndef DREAM_FREESTANDING
    char k[256];
    guest_path(key, k, sizeof(k));
    const char *v = getenv(k);
    if (!v) {
        return dream_intern_utf8("", 0);
    }
    size_t ln = strlen(v);
    char *enc = (char *)malloc(ln + 2);
    if (!enc) {
        return dream_intern_utf8("", 0);
    }
    enc[0] = '1';
    memcpy(enc + 1, v, ln);
    int32_t s = dream_intern_utf8(enc, (int32_t)ln + 1);
    free(enc);
    return s;
#else
    (void)key;
    return dream_intern_utf8("", 0);
#endif
}

void dream_process_env_set(int32_t key, int32_t val) {
#ifndef DREAM_FREESTANDING
    char k[256];
    char v[4096];
    guest_path(key, k, sizeof(k));
    guest_path(val, v, sizeof(v));
    setenv(k, v, 1);
#else
    (void)key;
    (void)val;
#endif
}

void dream_console_exit(int32_t code) {
#ifndef DREAM_FREESTANDING
    exit(code);
#else
    (void)code;
#endif
}

int32_t dream_console_read_line(void) {
#ifndef DREAM_FREESTANDING
    char buf[4096];
    if (!fgets(buf, sizeof(buf), stdin)) {
        return dream_intern_utf8("", 0);
    }
    size_t n = strlen(buf);
    if (n > 0 && buf[n - 1] == '\n') {
        buf[--n] = 0;
        if (n > 0 && buf[n - 1] == '\r') {
            buf[--n] = 0;
        }
    }
    return dream_intern_utf8(buf, (int32_t)n);
#else
    return dream_intern_utf8("", 0);
#endif
}

int32_t dream_console_read_key(void) {
#ifndef DREAM_FREESTANDING
    int c = getchar();
    return c == EOF ? 0 : (int32_t)(unsigned char)c;
#else
    return 0;
#endif
}

void dream_delay_ms(int32_t ms) {
#ifndef DREAM_FREESTANDING
    if (ms < 0) {
        ms = 0;
    }
    struct timespec ts;
    ts.tv_sec = (time_t)(ms / 1000);
    ts.tv_nsec = (long)(ms % 1000) * 1000000L;
    nanosleep(&ts, NULL);
#else
    (void)ms;
#endif
}
