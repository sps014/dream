#include "include/dream_host_support.h"

#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

char *dream_str_utf8(dream_ptr s) {
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

dream_ptr dream_str_from_utf8(const char *s) {
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

int32_t dream_host_path_is_dir(const char *path) {
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

void dream_host_names_free(char **names, size_t n) {
    size_t i;
    for (i = 0; i < n; i++) {
        free(names[i]);
    }
    free(names);
}

int dream_host_names_push(char ***names, size_t *n, size_t *cap, const char *name) {
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

dream_ptr dream_host_names_join_lines(char **names, size_t n) {
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
