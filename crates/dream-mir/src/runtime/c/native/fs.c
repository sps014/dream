#include "include/dream_host_support.h"

#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <stdio.h>
#include <sys/stat.h>

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
    int32_t ok = p && dream_host_path_is_dir(p);
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
    if (!src || !dst || dream_host_path_is_dir(src)) {
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
