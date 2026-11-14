#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#include "dream_process.h"
#include "dream_thread.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void spawn_message(char *message, size_t size, const char *prefix, const char *detail) {
    snprintf(message, size, "%s%s%s", prefix, detail && *detail ? ": " : "", detail ? detail : "");
}

static int env_key_valid(const char *key) {
    return key && *key && !strchr(key, '=');
}

#ifndef _WIN32

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>

#ifdef __APPLE__
#include <crt_externs.h>
#define dream_environ (*_NSGetEnviron())
#else
extern char **environ;
#define dream_environ environ
#endif

static dream_mutex spawn_mu = DREAM_MUTEX_INIT;

static int pipe_cloexec_locked(int fds[2]) {
#if defined(__linux__)
    return pipe2(fds, O_CLOEXEC);
#else
    if (pipe(fds) != 0) { return -1; }
    fcntl(fds[0], F_SETFD, FD_CLOEXEC);
    fcntl(fds[1], F_SETFD, FD_CLOEXEC);
    return 0;
#endif
}

int dream_process_pipe(int fds[2]) {
    dream_mutex_lock(&spawn_mu);
    int result = pipe_cloexec_locked(fds);
    dream_mutex_unlock(&spawn_mu);
    return result;
}

static void close_fd(int *fd) {
    if (*fd >= 0) { close(*fd); }
    *fd = -1;
}

static int key_matches(const char *entry, const char *key) {
    size_t length = strlen(key);
    return strncmp(entry, key, length) == 0 && entry[length] == '=';
}

/* Builds `KEY=VALUE` entries: the inherited environment minus overridden keys, then the
 * overrides. Only the joined override strings are owned; inherited entries are borrowed. */
static char **build_env(char *const *env, int32_t envc, int clear_env, char ***owned) {
    size_t inherited = 0;
    if (!clear_env) {
        while (dream_environ && dream_environ[inherited]) { ++inherited; }
    }
    int32_t pairs = envc / 2;
    char **entries = calloc(inherited + (size_t)pairs + 1, sizeof(char *));
    *owned = calloc((size_t)pairs + 1, sizeof(char *));
    if (!entries || !*owned) {
        free(entries);
        free(*owned);
        *owned = NULL;
        return NULL;
    }
    size_t count = 0;
    for (size_t i = 0; i < inherited; ++i) {
        int overridden = 0;
        for (int32_t p = 0; p < pairs && !overridden; ++p) {
            overridden = key_matches(dream_environ[i], env[p * 2]);
        }
        if (!overridden) { entries[count++] = dream_environ[i]; }
    }
    for (int32_t p = 0; p < pairs; ++p) {
        size_t key = strlen(env[p * 2]), value = strlen(env[p * 2 + 1]);
        char *entry = malloc(key + value + 2);
        if (!entry) { return entries; }
        memcpy(entry, env[p * 2], key);
        entry[key] = '=';
        memcpy(entry + key + 1, env[p * 2 + 1], value + 1);
        (*owned)[p] = entry;
        entries[count++] = entry;
    }
    return entries;
}

static void free_strings(char **strings) {
    if (!strings) { return; }
    for (size_t i = 0; strings[i]; ++i) { free(strings[i]); }
    free(strings);
}

/* Executable candidates are resolved before fork: the child may only call async-signal-safe
 * functions, so it walks this list with execve. */
static char **command_candidates(const char *command, char *const *env, int32_t envc) {
    if (strchr(command, '/')) {
        char **one = calloc(2, sizeof(char *));
        if (one) { one[0] = strdup(command); }
        if (one && !one[0]) { free(one); return NULL; }
        return one;
    }
    const char *path = NULL;
    for (int32_t p = 0; p + 1 < envc; p += 2) {
        if (strcmp(env[p], "PATH") == 0) { path = env[p + 1]; }
    }
    if (!path) { path = getenv("PATH"); }
    if (!path || !*path) { path = "/usr/local/bin:/usr/bin:/bin"; }
    size_t dirs = 1;
    for (const char *c = path; *c; ++c) { dirs += *c == ':'; }
    char **candidates = calloc(dirs + 1, sizeof(char *));
    if (!candidates) { return NULL; }
    size_t count = 0, command_length = strlen(command);
    const char *start = path;
    for (;;) {
        const char *end = strchr(start, ':');
        size_t length = end ? (size_t)(end - start) : strlen(start);
        const char *dir = length ? start : ".";
        size_t dir_length = length ? length : 1;
        char *candidate = malloc(dir_length + command_length + 2);
        if (!candidate) { free_strings(candidates); return NULL; }
        memcpy(candidate, dir, dir_length);
        candidate[dir_length] = '/';
        memcpy(candidate + dir_length + 1, command, command_length + 1);
        candidates[count++] = candidate;
        if (!end) { break; }
        start = end + 1;
    }
    return candidates;
}

static void child_fd(int fd, int target) {
    if (fd == target) {
        fcntl(target, F_SETFD, 0);
    } else {
        dup2(fd, target);
    }
}

typedef struct {
    int stage;
    int error;
} SpawnFailure;

static void child_fail(int report, int stage) {
    SpawnFailure failure = {stage, errno};
    ssize_t written = write(report, &failure, sizeof(failure));
    (void)written;
    _exit(127);
}

int dream_process_spawn(const char *command, char *const *args, int32_t argc, const char *cwd,
                        char *const *env, int32_t envc, int clear_env, DreamSpawned *out,
                        char *message, size_t message_size) {
    for (int32_t p = 0; p + 1 < envc; p += 2) {
        if (!env_key_valid(env[p])) {
            spawn_message(message, message_size, "invalid environment override", env[p]);
            return DREAM_SPAWN_INVALID;
        }
    }
    if (!*command) {
        spawn_message(message, message_size, "empty command", NULL);
        return DREAM_SPAWN_INVALID;
    }
    char **argv = calloc((size_t)argc + 2, sizeof(char *));
    char **owned_env = NULL;
    char **envp = build_env(env, envc, clear_env, &owned_env);
    char **candidates = command_candidates(command, env, envc);
    if (!argv || !envp || !candidates) {
        free(argv); free(envp); free_strings(owned_env); free_strings(candidates);
        spawn_message(message, message_size, "out of memory", NULL);
        return DREAM_SPAWN_FAILED;
    }
    argv[0] = (char *)command;
    for (int32_t i = 0; i < argc; ++i) { argv[i + 1] = args[i]; }

    int in[2] = {-1, -1}, output[2] = {-1, -1}, error[2] = {-1, -1}, report[2] = {-1, -1};
    int status = DREAM_SPAWN_OK;
    dream_mutex_lock(&spawn_mu);
    if (pipe_cloexec_locked(in) || pipe_cloexec_locked(output) || pipe_cloexec_locked(error)
        || pipe_cloexec_locked(report)) {
        spawn_message(message, message_size, "cannot create pipes", strerror(errno));
        status = DREAM_SPAWN_FAILED;
    }
    pid_t pid = -1;
    if (status == DREAM_SPAWN_OK) {
        pid = fork();
        if (pid < 0) {
            spawn_message(message, message_size, "cannot fork", strerror(errno));
            status = DREAM_SPAWN_FAILED;
        }
    }
    if (pid == 0) {
        struct sigaction action;
        memset(&action, 0, sizeof(action));
        action.sa_handler = SIG_DFL;
        sigaction(SIGPIPE, &action, NULL);
        sigset_t none;
        sigemptyset(&none);
        sigprocmask(SIG_SETMASK, &none, NULL);
        child_fd(in[0], 0);
        child_fd(output[1], 1);
        child_fd(error[1], 2);
        if (cwd && *cwd && chdir(cwd) != 0) { child_fail(report[1], 1); }
        int last = ENOENT;
        for (size_t i = 0; candidates[i]; ++i) {
            execve(candidates[i], argv, envp);
            if (errno != ENOENT && errno != ENOTDIR) {
                last = errno;
                if (errno != EACCES) { break; }
            }
        }
        errno = last;
        child_fail(report[1], 2);
    }
    dream_mutex_unlock(&spawn_mu);
    close_fd(&in[0]);
    close_fd(&output[1]);
    close_fd(&error[1]);
    close_fd(&report[1]);
    free(argv);
    free(envp);
    free_strings(owned_env);
    free_strings(candidates);

    if (status == DREAM_SPAWN_OK) {
        SpawnFailure failure;
        ssize_t got;
        do {
            got = read(report[0], &failure, sizeof(failure));
        } while (got < 0 && errno == EINTR);
        if (got == (ssize_t)sizeof(failure)) {
            int reaped;
            do {
                reaped = waitpid(pid, NULL, 0);
            } while (reaped < 0 && errno == EINTR);
            spawn_message(message, message_size,
                          failure.stage == 1 ? "cannot enter working directory" : "cannot launch command",
                          strerror(failure.error));
            status = DREAM_SPAWN_FAILED;
        }
    }
    close_fd(&report[0]);
    if (status != DREAM_SPAWN_OK) {
        close_fd(&in[1]);
        close_fd(&output[0]);
        close_fd(&error[0]);
        return status;
    }
    int parent[3] = {in[1], output[0], error[0]};
    for (int i = 0; i < 3; ++i) {
        fcntl(parent[i], F_SETFL, fcntl(parent[i], F_GETFL) | O_NONBLOCK);
        out->pipes[i] = parent[i];
    }
    out->pid = pid;
    return DREAM_SPAWN_OK;
}

#else

static wchar_t *wide(const char *text) {
    int size = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, NULL, 0);
    if (size <= 0) { return NULL; }
    wchar_t *out = malloc((size_t)size * sizeof(wchar_t));
    if (out && !MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, text, -1, out, size)) {
        free(out);
        return NULL;
    }
    return out;
}

typedef struct {
    wchar_t *data;
    size_t length, capacity;
} WideBuffer;

static int wide_push(WideBuffer *buffer, const wchar_t *text, size_t length) {
    if (buffer->length + length + 1 > buffer->capacity) {
        size_t capacity = (buffer->capacity ? buffer->capacity * 2 : 256) + length;
        wchar_t *grown = realloc(buffer->data, capacity * sizeof(wchar_t));
        if (!grown) { return 0; }
        buffer->data = grown;
        buffer->capacity = capacity;
    }
    memcpy(buffer->data + buffer->length, text, length * sizeof(wchar_t));
    buffer->length += length;
    buffer->data[buffer->length] = 0;
    return 1;
}

/* Quotes one argument so CommandLineToArgvW and the MSVC runtime recover it exactly. */
static int append_argument(WideBuffer *line, const wchar_t *arg) {
    if (line->length && !wide_push(line, L" ", 1)) { return 0; }
    if (*arg && !wcspbrk(arg, L" \t\n\v\"")) { return wide_push(line, arg, wcslen(arg)); }
    if (!wide_push(line, L"\"", 1)) { return 0; }
    for (const wchar_t *c = arg;; ++c) {
        size_t slashes = 0;
        while (*c == L'\\') { ++slashes; ++c; }
        size_t repeat = !*c ? slashes * 2 : *c == L'"' ? slashes * 2 + 1 : slashes;
        for (size_t i = 0; i < repeat; ++i) {
            if (!wide_push(line, L"\\", 1)) { return 0; }
        }
        if (!*c) { break; }
        if (!wide_push(line, c, 1)) { return 0; }
    }
    return wide_push(line, L"\"", 1);
}

static int wide_key_matches(const wchar_t *entry, const wchar_t *key) {
    size_t length = wcslen(key);
    return _wcsnicmp(entry, key, length) == 0 && entry[length] == L'=';
}

/* A double-NUL-terminated UTF-16 block: inherited entries minus overridden keys, then overrides. */
static wchar_t *build_env_block(char *const *env, int32_t envc, int clear_env) {
    int32_t pairs = envc / 2;
    wchar_t **keys = calloc((size_t)pairs + 1, sizeof(wchar_t *));
    WideBuffer block = {0};
    int ok = keys != NULL;
    for (int32_t p = 0; ok && p < pairs; ++p) { ok = (keys[p] = wide(env[p * 2])) != NULL; }
    wchar_t *inherited = ok && !clear_env ? GetEnvironmentStringsW() : NULL;
    for (const wchar_t *entry = inherited; ok && entry && *entry; entry += wcslen(entry) + 1) {
        int overridden = 0;
        for (int32_t p = 0; p < pairs && !overridden; ++p) { overridden = wide_key_matches(entry, keys[p]); }
        if (!overridden) { ok = wide_push(&block, entry, wcslen(entry) + 1); }
    }
    if (inherited) { FreeEnvironmentStringsW(inherited); }
    for (int32_t p = 0; ok && p < pairs; ++p) {
        wchar_t *value = wide(env[p * 2 + 1]);
        ok = value && wide_push(&block, keys[p], wcslen(keys[p])) && wide_push(&block, L"=", 1)
            && wide_push(&block, value, wcslen(value) + 1);
        free(value);
    }
    ok = ok && wide_push(&block, L"\0", 1);
    for (int32_t p = 0; keys && p < pairs; ++p) { free(keys[p]); }
    free(keys);
    if (!ok) {
        free(block.data);
        return NULL;
    }
    return block.data;
}

static void system_message(char *message, size_t size, const char *prefix, DWORD code) {
    char detail[256] = {0};
    FormatMessageA(FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS, NULL, code, 0, detail,
                   sizeof(detail) - 1, NULL);
    size_t length = strlen(detail);
    while (length && (detail[length - 1] == '\n' || detail[length - 1] == '\r')) { detail[--length] = 0; }
    spawn_message(message, size, prefix, detail);
}

static void close_handle(HANDLE *handle) {
    if (*handle) { CloseHandle(*handle); }
    *handle = NULL;
}

int dream_process_spawn(const char *command, char *const *args, int32_t argc, const char *cwd,
                        char *const *env, int32_t envc, int clear_env, DreamSpawned *out,
                        char *message, size_t message_size) {
    for (int32_t p = 0; p + 1 < envc; p += 2) {
        if (!env_key_valid(env[p])) {
            spawn_message(message, message_size, "invalid environment override", env[p]);
            return DREAM_SPAWN_INVALID;
        }
    }
    if (!*command) {
        spawn_message(message, message_size, "empty command", NULL);
        return DREAM_SPAWN_INVALID;
    }
    WideBuffer line = {0};
    int ok = 1;
    for (int32_t i = -1; ok && i < argc; ++i) {
        wchar_t *arg = wide(i < 0 ? command : args[i]);
        ok = arg && append_argument(&line, arg);
        free(arg);
    }
    wchar_t *directory = ok && cwd && *cwd ? wide(cwd) : NULL;
    int custom_env = clear_env || envc > 0;
    wchar_t *block = ok && custom_env ? build_env_block(env, envc, clear_env) : NULL;
    if (!ok || (cwd && *cwd && !directory) || (custom_env && !block)) {
        free(line.data); free(directory); free(block);
        spawn_message(message, message_size, "invalid command, argument, or environment text", NULL);
        return DREAM_SPAWN_INVALID;
    }

    SECURITY_ATTRIBUTES inherit = {sizeof(inherit), NULL, TRUE};
    HANDLE in[2] = {NULL, NULL}, output[2] = {NULL, NULL}, error[2] = {NULL, NULL};
    int status = DREAM_SPAWN_OK;
    if (!CreatePipe(&in[0], &in[1], &inherit, 0) || !CreatePipe(&output[0], &output[1], &inherit, 0)
        || !CreatePipe(&error[0], &error[1], &inherit, 0)) {
        system_message(message, message_size, "cannot create pipes", GetLastError());
        status = DREAM_SPAWN_FAILED;
    }
    HANDLE parent[3] = {in[1], output[0], error[0]};
    for (int i = 0; status == DREAM_SPAWN_OK && i < 3; ++i) {
        SetHandleInformation(parent[i], HANDLE_FLAG_INHERIT, 0);
    }

    /* The handle list keeps a concurrently spawned sibling from inheriting these pipe ends. */
    HANDLE inherited[3] = {in[0], output[1], error[1]};
    SIZE_T attribute_size = 0;
    LPPROC_THREAD_ATTRIBUTE_LIST attributes = NULL;
    if (status == DREAM_SPAWN_OK) {
        InitializeProcThreadAttributeList(NULL, 1, 0, &attribute_size);
        attributes = malloc(attribute_size);
        if (!attributes || !InitializeProcThreadAttributeList(attributes, 1, 0, &attribute_size)
            || !UpdateProcThreadAttribute(attributes, 0, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, inherited,
                                          sizeof(inherited), NULL, NULL)) {
            system_message(message, message_size, "cannot prepare process attributes", GetLastError());
            status = DREAM_SPAWN_FAILED;
        }
    }
    PROCESS_INFORMATION info;
    memset(&info, 0, sizeof(info));
    if (status == DREAM_SPAWN_OK) {
        STARTUPINFOEXW startup;
        memset(&startup, 0, sizeof(startup));
        startup.StartupInfo.cb = sizeof(startup);
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = in[0];
        startup.StartupInfo.hStdOutput = output[1];
        startup.StartupInfo.hStdError = error[1];
        startup.lpAttributeList = attributes;
        DWORD flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW;
        if (!CreateProcessW(NULL, line.data, NULL, NULL, TRUE, flags, block, directory,
                            &startup.StartupInfo, &info)) {
            system_message(message, message_size, "cannot launch command", GetLastError());
            status = DREAM_SPAWN_FAILED;
        }
    }
    if (attributes) {
        DeleteProcThreadAttributeList(attributes);
        free(attributes);
    }
    free(line.data);
    free(directory);
    free(block);
    close_handle(&in[0]);
    close_handle(&output[1]);
    close_handle(&error[1]);
    if (status != DREAM_SPAWN_OK) {
        close_handle(&in[1]);
        close_handle(&output[0]);
        close_handle(&error[0]);
        return status;
    }
    CloseHandle(info.hThread);
    out->process = info.hProcess;
    out->pipes[0] = in[1];
    out->pipes[1] = output[0];
    out->pipes[2] = error[0];
    return DREAM_SPAWN_OK;
}

#endif
