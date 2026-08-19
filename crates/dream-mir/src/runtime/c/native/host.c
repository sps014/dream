#include "include/dream_rt_native.h"

#include <errno.h>
#include <limits.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#ifdef __APPLE__
#include <mach-o/dyld.h>
#endif

#ifdef _WIN32
#include <conio.h>
#include <direct.h>
#include <io.h>
#include <sys/stat.h>
#include <windows.h>
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
double dream_host_log(double v) { return log(v); }
double dream_host_log10(double v) { return log10(v); }
double dream_host_exp(double v) { return exp(v); }
double dream_host_hypot(double x, double y) { return hypot(x, y); }

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
#ifndef _WIN32
        struct stat st;
        ok = stat(p, &st) == 0;
#else
        struct _stat st;
        ok = _stat(p, &st) == 0;
#endif
        free(p);
    }
    return ok;
}

int64_t fileSize(dream_ptr path) {
    char *text = dream_str_utf8(path);
    int64_t size = -1;
#ifndef _WIN32
    struct stat st;
    if (text && stat(text, &st) == 0) {
        size = (int64_t)st.st_size;
    }
#else
    struct _stat st;
    if (text && _stat(text, &st) == 0) {
        size = (int64_t)st.st_size;
    }
#endif
    free(text);
    return size;
}

static int32_t path_is_dir(const char *path) {
#ifdef _WIN32
    struct _stat st;
    return path && _stat(path, &st) == 0 && (st.st_mode & _S_IFDIR) != 0;
#else
    struct stat st;
    return path && stat(path, &st) == 0 && S_ISDIR(st.st_mode);
#endif
}

static int cmp_cstr(const void *a, const void *b) {
    return strcmp(*(char *const *)a, *(char *const *)b);
}

static void names_free(char **names, size_t n) {
    size_t i;
    for (i = 0; i < n; i++) {
        free(names[i]);
    }
    free(names);
}

static int names_push(char ***names, size_t *n, size_t *cap, const char *name) {
    char *copy;
    char **grown;
    if (*n == *cap) {
        size_t next = *cap == 0 ? 8 : *cap * 2;
        grown = (char **)realloc(*names, next * sizeof(char *));
        if (!grown) {
            return 0;
        }
        *names = grown;
        *cap = next;
    }
    copy = (char *)malloc(strlen(name) + 1);
    if (!copy) {
        return 0;
    }
    memcpy(copy, name, strlen(name) + 1);
    (*names)[(*n)++] = copy;
    return 1;
}

static dream_ptr names_join_lines(char **names, size_t n) {
    size_t i;
    size_t total = 0;
    char *joined;
    dream_ptr out;
    if (n == 0) {
        return dream_str_from_utf8("");
    }
    qsort(names, n, sizeof(char *), cmp_cstr);
    for (i = 0; i < n; i++) {
        total += strlen(names[i]);
        if (i + 1 < n) {
            total += 1;
        }
    }
    joined = (char *)malloc(total + 1);
    if (!joined) {
        return dream_str_from_utf8("");
    }
    joined[0] = 0;
    for (i = 0; i < n; i++) {
        if (i > 0) {
            strcat(joined, "\n");
        }
        strcat(joined, names[i]);
    }
    out = dream_str_from_utf8(joined);
    free(joined);
    return out;
}

dream_ptr fileReadBytes(dream_ptr path) {
    char *p = dream_str_utf8(path);
    FILE *f;
    long sz;
    dream_ptr out;
    size_t nread;
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
    if (sz < 0 || sz > (long)(INT32_MAX - 4)) {
        fclose(f);
        return 0;
    }
    out = dream_array_new((int32_t)sz, 1);
    nread = fread((char *)dream_p(out) + 4, 1, (size_t)sz, f);
    fclose(f);
    dream_i32(out)[0] = (int32_t)nread;
    return out;
}

int64_t fileWriteBytes(dream_ptr path, dream_ptr data) {
    char *p = dream_str_utf8(path);
    FILE *f;
    int32_t n;
    int64_t written = -1;
    if (!p) {
        return -1;
    }
    n = data ? dream_i32(data)[0] : 0;
    f = fopen(p, "wb");
    free(p);
    if (f) {
        written = (int64_t)fwrite(data && n > 0 ? (char *)dream_p(data) + 4 : "", 1, (size_t)n, f);
        fclose(f);
    }
    return written;
}

int32_t fileIsDir(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = p && path_is_dir(p);
    free(p);
    return ok;
}

static int32_t path_kind_and_times(
    const char *path,
    int64_t *size,
    int64_t *mtime_ms,
    int64_t *ctime_ms,
    int64_t *atime_ms,
    int32_t *mode,
    int32_t *kind
) {
#ifdef _WIN32
    struct _stat st;
    if (!path || _stat(path, &st) != 0) {
        return 0;
    }
    *size = (int64_t)st.st_size;
    *mtime_ms = (int64_t)st.st_mtime * 1000;
    *ctime_ms = (int64_t)st.st_ctime * 1000;
    *atime_ms = (int64_t)st.st_atime * 1000;
    *mode = 0;
    if (st.st_mode & _S_IFDIR) {
        *kind = 1;
    } else if (st.st_mode & _S_IFREG) {
        *kind = 0;
    } else {
        *kind = 3;
    }
#else
    struct stat st;
    if (!path || stat(path, &st) != 0) {
        return 0;
    }
    *size = (int64_t)st.st_size;
#if defined(__APPLE__)
    *mtime_ms = (int64_t)st.st_mtimespec.tv_sec * 1000 + (int64_t)st.st_mtimespec.tv_nsec / 1000000;
    *ctime_ms = (int64_t)st.st_ctimespec.tv_sec * 1000 + (int64_t)st.st_ctimespec.tv_nsec / 1000000;
    *atime_ms = (int64_t)st.st_atimespec.tv_sec * 1000 + (int64_t)st.st_atimespec.tv_nsec / 1000000;
#else
    *mtime_ms = (int64_t)st.st_mtim.tv_sec * 1000 + (int64_t)st.st_mtim.tv_nsec / 1000000;
    *ctime_ms = (int64_t)st.st_ctim.tv_sec * 1000 + (int64_t)st.st_ctim.tv_nsec / 1000000;
    *atime_ms = (int64_t)st.st_atim.tv_sec * 1000 + (int64_t)st.st_atim.tv_nsec / 1000000;
#endif
    *mode = (int32_t)st.st_mode;
    if (S_ISREG(st.st_mode)) {
        *kind = 0;
    } else if (S_ISDIR(st.st_mode)) {
        *kind = 1;
    } else if (S_ISLNK(st.st_mode)) {
        *kind = 2;
    } else {
        *kind = 3;
    }
#endif
    return 1;
}

dream_ptr fileStat(dream_ptr path) {
    char *p = dream_str_utf8(path);
    char buf[192];
    int64_t size = 0;
    int64_t mtime_ms = 0;
    int64_t ctime_ms = 0;
    int64_t atime_ms = 0;
    int32_t mode = 0;
    int32_t kind = 3;
    dream_ptr out;
    if (!p || !path_kind_and_times(p, &size, &mtime_ms, &ctime_ms, &atime_ms, &mode, &kind)) {
        free(p);
        return dream_str_from_utf8("");
    }
    free(p);
    snprintf(
        buf,
        sizeof(buf),
        "%lld\n%lld\n%lld\n%lld\n%d\n%d",
        (long long)size,
        (long long)mtime_ms,
        (long long)ctime_ms,
        (long long)atime_ms,
        mode,
        kind
    );
    out = dream_str_from_utf8(buf);
    return out;
}

int32_t fileCopy(dream_ptr from, dream_ptr to) {
    char *src = dream_str_utf8(from);
    char *dst = dream_str_utf8(to);
    FILE *in;
    FILE *out;
    char buf[8192];
    size_t n;
    int32_t ok = 0;
    if (!src || !dst || path_is_dir(src)) {
        free(src);
        free(dst);
        return 0;
    }
    in = fopen(src, "rb");
    if (!in) {
        free(src);
        free(dst);
        return 0;
    }
    out = fopen(dst, "wb");
    if (!out) {
        fclose(in);
        free(src);
        free(dst);
        return 0;
    }
    ok = 1;
    while ((n = fread(buf, 1, sizeof(buf), in)) > 0) {
        if (fwrite(buf, 1, n, out) != n) {
            ok = 0;
            break;
        }
    }
    fclose(in);
    fclose(out);
    free(src);
    free(dst);
    return ok;
}

int32_t fileRename(dream_ptr from, dream_ptr to) {
    char *src = dream_str_utf8(from);
    char *dst = dream_str_utf8(to);
    int32_t ok = src && dst && rename(src, dst) == 0;
    free(src);
    free(dst);
    return ok;
}

int32_t dirRemove(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = 0;
    if (p) {
#ifdef _WIN32
        ok = _rmdir(p) == 0;
#else
        ok = rmdir(p) == 0;
#endif
        free(p);
    }
    return ok;
}

static int32_t dir_remove_all_path(const char *path);

#ifdef _WIN32
static int32_t dir_remove_all_path(const char *path) {
    char pattern[4096];
    WIN32_FIND_DATAA fd;
    HANDLE h;
    if (!path_is_dir(path)) {
        return remove(path) == 0;
    }
    snprintf(pattern, sizeof(pattern), "%s\\*", path);
    h = FindFirstFileA(pattern, &fd);
    if (h != INVALID_HANDLE_VALUE) {
        do {
            char child[4096];
            if (strcmp(fd.cFileName, ".") == 0 || strcmp(fd.cFileName, "..") == 0) {
                continue;
            }
            snprintf(child, sizeof(child), "%s\\%s", path, fd.cFileName);
            if (!dir_remove_all_path(child)) {
                FindClose(h);
                return 0;
            }
        } while (FindNextFileA(h, &fd));
        FindClose(h);
    }
    return _rmdir(path) == 0;
}
#else
static int32_t dir_remove_all_path(const char *path) {
    DIR *dir;
    struct dirent *ent;
    if (!path_is_dir(path)) {
        return remove(path) == 0;
    }
    dir = opendir(path);
    if (!dir) {
        return 0;
    }
    while ((ent = readdir(dir)) != NULL) {
        char child[4096];
        if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) {
            continue;
        }
        snprintf(child, sizeof(child), "%s/%s", path, ent->d_name);
        if (!dir_remove_all_path(child)) {
            closedir(dir);
            return 0;
        }
    }
    closedir(dir);
    return rmdir(path) == 0;
}
#endif

int32_t dirRemoveAll(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = p && dir_remove_all_path(p);
    free(p);
    return ok;
}

dream_ptr dirList(dream_ptr path) {
    char *p = dream_str_utf8(path);
    char **names = NULL;
    size_t n = 0;
    size_t cap = 0;
    dream_ptr out;
    if (!p || !path_is_dir(p)) {
        free(p);
        return dream_str_from_utf8("");
    }
#ifdef _WIN32
    {
        char pattern[4096];
        WIN32_FIND_DATAA fd;
        HANDLE h;
        snprintf(pattern, sizeof(pattern), "%s\\*", p);
        h = FindFirstFileA(pattern, &fd);
        if (h == INVALID_HANDLE_VALUE) {
            free(p);
            return dream_str_from_utf8("");
        }
        do {
            if (strcmp(fd.cFileName, ".") == 0 || strcmp(fd.cFileName, "..") == 0) {
                continue;
            }
            if (!names_push(&names, &n, &cap, fd.cFileName)) {
                names_free(names, n);
                FindClose(h);
                free(p);
                return dream_str_from_utf8("");
            }
        } while (FindNextFileA(h, &fd));
        FindClose(h);
    }
#else
    {
        DIR *dir = opendir(p);
        struct dirent *ent;
        if (!dir) {
            free(p);
            return dream_str_from_utf8("");
        }
        while ((ent = readdir(dir)) != NULL) {
            if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0) {
                continue;
            }
            if (!names_push(&names, &n, &cap, ent->d_name)) {
                names_free(names, n);
                closedir(dir);
                free(p);
                return dream_str_from_utf8("");
            }
        }
        closedir(dir);
    }
#endif
    free(p);
    out = names_join_lines(names, n);
    names_free(names, n);
    return out;
}

int32_t dirCreate(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = 0;
    if (p) {
#ifdef _WIN32
        ok = _mkdir(p) == 0;
#else
        ok = mkdir(p, 0755) == 0;
#endif
        free(p);
    }
    return ok;
}

static int32_t dir_create_all_utf8(char *path) {
    char *p;
    if (!path || !*path) {
        return 0;
    }
    p = path;
#ifdef _WIN32
    if (((p[0] >= 'A' && p[0] <= 'Z') || (p[0] >= 'a' && p[0] <= 'z')) && p[1] == ':') {
        p += 2;
    }
#endif
    if (*p == '/' || *p == '\\') {
        p += 1;
    }
    for (; *p; p++) {
        if (*p == '/' || *p == '\\') {
            char saved = *p;
            *p = 0;
            if (!path_is_dir(path)) {
#ifdef _WIN32
                if (_mkdir(path) != 0 && !path_is_dir(path)) {
                    *p = saved;
                    return 0;
                }
#else
                if (mkdir(path, 0755) != 0 && !path_is_dir(path)) {
                    *p = saved;
                    return 0;
                }
#endif
            }
            *p = saved;
        }
    }
    if (path_is_dir(path)) {
        return 1;
    }
#ifdef _WIN32
    return _mkdir(path) == 0 || path_is_dir(path);
#else
    return mkdir(path, 0755) == 0 || path_is_dir(path);
#endif
}

int32_t dirCreateAll(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = p && dir_create_all_utf8(p);
    free(p);
    return ok;
}

int32_t processPlatform(void) { return 0; }

int32_t processOsFamily(void) {
#ifdef _WIN32
    return 1;
#else
    return 0;
#endif
}

static char *dream_captured_args;
static char dream_exe_path_buf[4096];

void dream_process_capture_args(int32_t argc, char **argv) {
    int32_t i;
    size_t total = 0;
    char *join;
    if (argc > 0 && argv && argv[0]) {
#ifdef __APPLE__
        uint32_t n = sizeof(dream_exe_path_buf);
        if (_NSGetExecutablePath(dream_exe_path_buf, &n) != 0) {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#elif defined(_WIN32)
        if (!GetModuleFileNameA(NULL, dream_exe_path_buf, sizeof(dream_exe_path_buf))) {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#else
        ssize_t n = readlink("/proc/self/exe", dream_exe_path_buf, sizeof(dream_exe_path_buf) - 1);
        if (n > 0) {
            dream_exe_path_buf[n] = 0;
        } else {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#endif
        dream_exe_path_buf[sizeof(dream_exe_path_buf) - 1] = 0;
    }
    for (i = 1; i < argc; i++) {
        total += strlen(argv[i]);
        if (i + 1 < argc) {
            total += 1;
        }
    }
    free(dream_captured_args);
    dream_captured_args = (char *)malloc(total + 1);
    if (!dream_captured_args) {
        return;
    }
    dream_captured_args[0] = 0;
    for (i = 1; i < argc; i++) {
        if (i > 1) {
            strcat(dream_captured_args, "\n");
        }
        strcat(dream_captured_args, argv[i]);
    }
}

dream_ptr processArgs(void) {
    return dream_str_from_utf8(dream_captured_args ? dream_captured_args : "");
}

dream_ptr processExePath(void) {
    return dream_str_from_utf8(dream_exe_path_buf);
}

dream_ptr consoleReadLine(void) {
    char buf[4096];
    size_t n;
    if (!fgets(buf, sizeof(buf), stdin)) {
        return dream_str_from_utf8("");
    }
    n = strlen(buf);
    if (n > 0 && buf[n - 1] == '\n') {
        buf[n - 1] = 0;
        n--;
        if (n > 0 && buf[n - 1] == '\r') {
            buf[n - 1] = 0;
        }
    }
    return dream_str_from_utf8(buf);
}

int32_t consoleReadKey(void) {
#ifdef _WIN32
    return _getch();
#else
    return fgetc(stdin);
#endif
}

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

void processEnvUnset(dream_ptr name) {
    char *key = dream_str_utf8(name);
    if (key) {
#ifdef _WIN32
        _putenv_s(key, "");
#else
        unsetenv(key);
#endif
        free(key);
    }
}

dream_ptr processEnvKeys(void) {
#ifdef _WIN32
    char **env = _environ;
#else
    extern char **environ;
    char **env = environ;
#endif
    char **names = NULL;
    size_t n = 0;
    size_t cap = 0;
    size_t i;
    dream_ptr out;
    if (!env) {
        return dream_str_from_utf8("");
    }
    for (i = 0; env[i]; i++) {
        const char *eq = strchr(env[i], '=');
        char key[512];
        size_t len = eq ? (size_t)(eq - env[i]) : strlen(env[i]);
        if (len >= sizeof(key)) {
            len = sizeof(key) - 1;
        }
        memcpy(key, env[i], len);
        key[len] = 0;
        if (!names_push(&names, &n, &cap, key)) {
            names_free(names, n);
            return dream_str_from_utf8("");
        }
    }
    out = names_join_lines(names, n);
    names_free(names, n);
    return out;
}

dream_ptr processTempDir(void) {
#ifdef _WIN32
    char path[MAX_PATH];
    DWORD len = GetTempPathA(MAX_PATH, path);
    if (len == 0 || len >= MAX_PATH) {
        return dream_str_from_utf8("");
    }
    if (len > 0 && (path[len - 1] == '\\' || path[len - 1] == '/')) {
        path[len - 1] = 0;
    }
    return dream_str_from_utf8(path);
#else
    const char *t = getenv("TMPDIR");
    if (!t || !*t) {
        t = getenv("TMP");
    }
    if (!t || !*t) {
        t = "/tmp";
    }
    return dream_str_from_utf8(t);
#endif
}

dream_ptr processHomeDir(void) {
    const char *h = getenv("HOME");
    if (!h || !*h) {
        h = getenv("USERPROFILE");
    }
    if (!h || !*h) {
        return dream_str_from_utf8("");
    }
    return dream_str_from_utf8(h);
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

int64_t fileHandleTell(int32_t fd) {
    off_t pos = lseek(fd, 0, SEEK_CUR);
    return pos < 0 ? -1 : (int64_t)pos;
}

int32_t fileHandleSeekEnd(int32_t fd, int64_t offset) {
    return lseek(fd, (off_t)offset, SEEK_END) < 0 ? -1 : 0;
}

void fileHandleClose(int32_t fd) {
    if (fd >= 0) {
        close(fd);
    }
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

void consoleExit(int32_t code) { exit(code); }

