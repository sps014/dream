#include "dream_host_support.h"

#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <stdio.h>
#include <sys/stat.h>

#ifdef _WIN32
#include <direct.h>
#include <io.h>
#include "dream_host_windows.h"
#else
#include <unistd.h>
#endif

int32_t fileDelete(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = 0;
    if (p) {
#ifdef _WIN32
        ok = dream_host_path_is_dir(p) ? _rmdir(p) == 0 : remove(p) == 0;
#else
        ok = remove(p) == 0;
#endif
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
    FILE *f = p && !dream_host_path_is_dir(p) ? fopen(p, "rb") : NULL;
    free(p);
    if (!f) {
        return dream_status_bytes(1, NULL, 0);
    }
    long size = fseek(f, 0, SEEK_END) == 0 ? ftell(f) : -1;
    if (size < 0 || size >= INT32_MAX || fseek(f, 0, SEEK_SET) != 0) {
        fclose(f);
        return dream_status_bytes(1, NULL, 0);
    }
    dream_ptr out = dream_status_buffer((int32_t)size);
    size_t read = fread(dream_array_bytes(out) + 1, 1, (size_t)size, f);
    int failed = ferror(f);
    if (fclose(f) != 0) { failed = 1; }
    dream_status_finish(out, failed ? -1 : (int64_t)read);
    return out;
}

static int64_t file_write_bytes(dream_ptr path, dream_ptr data, const char *mode) {
    char *p = dream_str_utf8(path);
    FILE *f = p ? fopen(p, mode) : NULL;
    free(p);
    if (!f) { return -1; }
    int32_t count = data ? dream_i32(data)[0] : 0;
    size_t written = count ? fwrite(dream_array_bytes(data), 1, (size_t)count, f) : 0;
    return fclose(f) == 0 && written == (size_t)count ? (int64_t)written : -1;
}

int64_t fileAppend(dream_ptr path, dream_ptr data) {
    return file_write_bytes(path, data, "ab");
}

int64_t fileWriteBytes(dream_ptr path, dream_ptr data) {
    return file_write_bytes(path, data, "wb");
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

/* Compare the opened files before truncating the destination, including hard-link aliases. */
static int copy_prepare(FILE *in, FILE *out) {
#ifdef _WIN32
    int fd = _fileno(out);
    BY_HANDLE_FILE_INFORMATION src, dst;
    if (!GetFileInformationByHandle((HANDLE)_get_osfhandle(_fileno(in)), &src) ||
        !GetFileInformationByHandle((HANDLE)_get_osfhandle(fd), &dst)) { return -1; }
    if (src.dwVolumeSerialNumber == dst.dwVolumeSerialNumber &&
        src.nFileIndexHigh == dst.nFileIndexHigh && src.nFileIndexLow == dst.nFileIndexLow) { return 0; }
    return _chsize_s(fd, 0) == 0 ? 1 : -1;
#else
    int fd = fileno(out);
    struct stat src, dst;
    if (fstat(fileno(in), &src) != 0 || fstat(fd, &dst) != 0) { return -1; }
    if (src.st_dev == dst.st_dev && src.st_ino == dst.st_ino) { return 0; }
    return ftruncate(fd, 0) == 0 ? 1 : -1;
#endif
}

int32_t fileCopy(dream_ptr from, dream_ptr to) {
    char *src = dream_str_utf8(from);
    char *dst = dream_str_utf8(to);
    FILE *in = src && dst && !dream_host_path_is_dir(src) ? fopen(src, "rb") : NULL;
    FILE *out = in ? fopen(dst, "ab") : NULL;
    free(src);
    free(dst);
    if (!out) {
        if (in) { fclose(in); }
        return 0;
    }
    int prepare = copy_prepare(in, out);
    int ok = prepare >= 0;
    if (prepare > 0) {
        char buf[8192];
        size_t n;
        while ((n = fread(buf, 1, sizeof(buf), in)) > 0) {
            if (fwrite(buf, 1, n, out) != n) {
                ok = 0;
                break;
            }
        }
        if (ferror(in)) { ok = 0; }
    }
    fclose(in);
    return fclose(out) == 0 && ok;
}

int32_t fileRename(dream_ptr from, dream_ptr to) {
    char *src = dream_str_utf8(from);
    char *dst = dream_str_utf8(to);
    int32_t ok = src && dst && rename(src, dst) == 0;
    free(src);
    free(dst);
    return ok;
}
