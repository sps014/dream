#include "include/dream_host_support.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <sys/stat.h>

#ifdef _WIN32
#include "include/dream_host_windows.h"
#include <direct.h>
#else
#include <dirent.h>
#include <unistd.h>
#endif

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
    if (!dream_host_path_is_dir(path)) {
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
    if (!dream_host_path_is_dir(path)) {
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
    if (!p || !dream_host_path_is_dir(p)) {
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
            if (!dream_host_names_push(&names, &n, &cap, fd.cFileName)) {
                dream_host_names_free(names, n);
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
            if (!dream_host_names_push(&names, &n, &cap, ent->d_name)) {
                dream_host_names_free(names, n);
                closedir(dir);
                free(p);
                return dream_str_from_utf8("");
            }
        }
        closedir(dir);
    }
#endif
    free(p);
    out = dream_host_names_join_lines(names, n);
    dream_host_names_free(names, n);
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
            if (!dream_host_path_is_dir(path)) {
#ifdef _WIN32
                if (_mkdir(path) != 0 && !dream_host_path_is_dir(path)) {
                    *p = saved;
                    return 0;
                }
#else
                if (mkdir(path, 0755) != 0 && !dream_host_path_is_dir(path)) {
                    *p = saved;
                    return 0;
                }
#endif
            }
            *p = saved;
        }
    }
    if (dream_host_path_is_dir(path)) {
        return 1;
    }
#ifdef _WIN32
    return _mkdir(path) == 0 || dream_host_path_is_dir(path);
#else
    return mkdir(path, 0755) == 0 || dream_host_path_is_dir(path);
#endif
}

int32_t dirCreateAll(dream_ptr path) {
    char *p = dream_str_utf8(path);
    int32_t ok = p && dir_create_all_utf8(p);
    free(p);
    return ok;
}
