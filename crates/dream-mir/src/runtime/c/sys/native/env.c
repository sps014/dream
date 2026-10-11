#include "dream_host_support.h"

#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include "dream_host_windows.h"
#endif

/* A set variable comes back as "1" + value so an empty value stays distinct from unset. */
#ifdef _WIN32
/* The CRT's getenv locks and compares every entry case-insensitively through the locale;
 * Win32 answers directly in the UTF-16 strings already use. */
dream_ptr processEnvGet(dream_ptr name) {
    int32_t n = dream_str_len(name);
    wchar_t *key = malloc(((size_t)n + 1) * sizeof(wchar_t));
    dream_ptr result = 0;
    if (!key) {
        return 0;
    }
    memcpy(key, dream_str_units(name), (size_t)n * sizeof(wchar_t));
    key[n] = 0;
    wchar_t inline_value[256];
    wchar_t *value = inline_value;
    DWORD cap = sizeof(inline_value) / sizeof(inline_value[0]);
    SetLastError(ERROR_SUCCESS);
    DWORD len = GetEnvironmentVariableW(key, value, cap);
    /* A too-small buffer reports the size it needs, terminator included. */
    while (len >= cap) {
        if (value != inline_value) {
            free(value);
        }
        cap = len;
        value = malloc((size_t)cap * sizeof(wchar_t));
        if (!value) {
            free(key);
            return 0;
        }
        len = GetEnvironmentVariableW(key, value, cap);
    }
    if (len != 0 || GetLastError() != ERROR_ENVVAR_NOT_FOUND) {
        result = dream_string_alloc((int32_t)len + 1);
        uint16_t *units = (uint16_t *)((char *)dream_p(result) + STRING_UNITS_OFFSET);
        units[0] = '1';
        memcpy(units + 1, value, (size_t)len * sizeof(wchar_t));
    }
    if (value != inline_value) {
        free(value);
    }
    free(key);
    return result;
}
#else
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
#endif

void processEnvSet(dream_ptr name, dream_ptr value) {
    char *key = dream_str_utf8(name);
    char *text = dream_str_utf8(value);
    if (key && text) {
#ifdef _WIN32
        _putenv_s(key, text);
#else
        setenv(key, text, 1);
#endif
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
        if (!dream_host_names_push(&names, &n, &cap, key)) {
            dream_host_names_free(names, n);
            return dream_str_from_utf8("");
        }
    }
    out = dream_host_names_join_lines(names, n);
    dream_host_names_free(names, n);
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
