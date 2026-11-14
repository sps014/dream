#include "dream_host_support.h"
#include "dream_process.h"
#include "dream_thread.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef _WIN32
#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>
#endif

/* One lock guards the child table and every child and call field; blocked pipe I/O runs on a
 * dedicated thread per call so a quiet child never occupies the shared blocking executor. */

enum { CALL_SPAWN, CALL_READ, CALL_WRITE, CALL_WAIT };
enum { STREAM_STDIN, STREAM_STDOUT, STREAM_STDERR };

#define PROCESS_READ_LIMIT (16 * 1024 * 1024)

typedef struct ProcessCall ProcessCall;

typedef struct Child {
    unsigned references;
    int32_t id;
    DreamSpawned spawned;
    int exited, killed, stdin_close;
    int32_t status;
    uint8_t *leftover[3];
    int32_t leftover_length[3];
    dream_mutex stream_mu[3];
    ProcessCall *calls;
    struct Child *next;
} Child;

struct ProcessCall {
    unsigned references;
    int kind, stream, done, claimed, cancelled;
    Child *child;
    int64_t value;
    uint8_t *bytes;
    int32_t count;
    const char *code;
    char *message;
    dream_ptr waiter;
    ProcessCall *next;
#ifdef _WIN32
    HANDLE thread;
#else
    int wake[2];
#endif
};

static dream_mutex process_mu = DREAM_MUTEX_INIT;
static Child *children;
static int32_t next_child_id = 1;

static Child *child_find_locked(int32_t id) {
    for (Child *child = children; child; child = child->next) {
        if (child->id == id) { return child; }
    }
    return NULL;
}

static void child_unlink_locked(Child *child) {
    for (Child **link = &children; *link; link = &(*link)->next) {
        if (*link == child) {
            *link = child->next;
            return;
        }
    }
}

static void pipe_close(dream_pipe *pipe) {
    if (*pipe == DREAM_PIPE_NONE) { return; }
#ifdef _WIN32
    CloseHandle(*pipe);
#else
    close(*pipe);
#endif
    *pipe = DREAM_PIPE_NONE;
}

static void child_drop_locked(Child *child) {
    if (--child->references) { return; }
    for (int i = 0; i < 3; ++i) {
        pipe_close(&child->spawned.pipes[i]);
        free(child->leftover[i]);
        dream_mutex_destroy(&child->stream_mu[i]);
    }
#ifdef _WIN32
    CloseHandle(child->spawned.process);
#endif
    free(child);
}

static void child_kill_locked(Child *child) {
    if (child->exited || child->killed) { return; }
    child->killed = 1;
#ifdef _WIN32
    TerminateProcess(child->spawned.process, 1);
#else
    kill(child->spawned.pid, SIGKILL);
#endif
}

static void call_unlink_locked(ProcessCall *call) {
    if (!call->child) { return; }
    for (ProcessCall **link = &call->child->calls; *link; link = &(*link)->next) {
        if (*link == call) {
            *link = call->next;
            break;
        }
    }
    call->next = NULL;
}

static int call_writing_locked(Child *child) {
    for (ProcessCall *call = child->calls; call; call = call->next) {
        if (call->kind == CALL_WRITE) { return 1; }
    }
    return 0;
}

static void stdin_close_locked(Child *child) {
    if (child->stdin_close && !call_writing_locked(child)) {
        pipe_close(&child->spawned.pipes[STREAM_STDIN]);
    }
}

/* Bytes a cancelled or abandoned read already took from the pipe go back in front of the
 * stream, so the next read still sees them in order. */
static void leftover_prepend_locked(Child *child, int stream, const uint8_t *bytes, int32_t count) {
    if (count <= 0) { return; }
    int32_t old = child->leftover_length[stream];
    uint8_t *joined = malloc((size_t)old + (size_t)count);
    if (!joined) { return; }
    memcpy(joined, bytes, (size_t)count);
    if (old) { memcpy(joined + count, child->leftover[stream], (size_t)old); }
    free(child->leftover[stream]);
    child->leftover[stream] = joined;
    child->leftover_length[stream] = old + count;
}

static void call_free_locked(ProcessCall *call) {
    Child *child = call->child;
    if (call->kind == CALL_READ && call->done && !call->claimed && !call->code && call->value > 0) {
        leftover_prepend_locked(child, call->stream, call->bytes, (int32_t)call->value);
    }
    if (call->kind == CALL_SPAWN && call->done && !call->claimed && !call->code) {
        Child *spawned = child_find_locked((int32_t)call->value);
        if (spawned) {
            child_kill_locked(spawned);
            child_unlink_locked(spawned);
            child_drop_locked(spawned);
        }
    }
    if (child && call->kind != CALL_SPAWN) { child_drop_locked(child); }
#ifdef _WIN32
    if (call->thread) { CloseHandle(call->thread); }
#else
    if (call->wake[0] >= 0) { close(call->wake[0]); }
    if (call->wake[1] >= 0) { close(call->wake[1]); }
#endif
    free(call->bytes);
    free(call->message);
    free(call);
}

static void call_drop(ProcessCall *call) {
    dream_mutex_lock(&process_mu);
    if (!--call->references) { call_free_locked(call); }
    dream_mutex_unlock(&process_mu);
}

static ProcessCall *call_new(int kind, Child *child, int stream) {
    ProcessCall *call = calloc(1, sizeof(*call));
    if (!call) { return NULL; }
    call->references = 1;
    call->kind = kind;
    call->stream = stream;
    call->child = child;
    call->value = -1;
#ifndef _WIN32
    call->wake[0] = call->wake[1] = -1;
#endif
    return call;
}

static void call_set_error_locked(ProcessCall *call, const char *code, const char *message) {
    call->code = code;
    free(call->message);
    call->message = strdup(message ? message : "");
}

/* Settles the call and returns its waiter, which the caller completes outside the lock. */
static dream_ptr call_settle_locked(ProcessCall *call, int64_t value, const char *code, const char *message) {
    if (call->done) { return 0; }
    call->done = 1;
    call->value = value;
    if (code) { call_set_error_locked(call, code, message); }
    call_unlink_locked(call);
    if (call->kind == CALL_WRITE && call->child) { stdin_close_locked(call->child); }
    dream_ptr waiter = call->waiter;
    call->waiter = 0;
    return waiter;
}

static void call_settle(ProcessCall *call, int64_t value, const char *code, const char *message) {
    dream_mutex_lock(&process_mu);
    dream_ptr waiter = call_settle_locked(call, value, code, message);
    dream_mutex_unlock(&process_mu);
    if (waiter) { dream_complete_foreign(waiter, 0); }
}

static void call_wake_locked(ProcessCall *call) {
#ifdef _WIN32
    if (call->kind == CALL_WRITE && call->thread) { CancelSynchronousIo(call->thread); }
#else
    if (call->wake[1] >= 0) {
        char byte = 1;
        ssize_t written = write(call->wake[1], &byte, 1);
        (void)written;
    }
#endif
}

/* Read and write calls end promptly with `code`; exit waits keep waiting for the reaper. */
static void child_interrupt_locked(Child *child, const char *code, const char *message) {
    for (ProcessCall *call = child->calls; call; call = call->next) {
        if (call->kind == CALL_WAIT || call->cancelled) { continue; }
        call->cancelled = 1;
        call_set_error_locked(call, code, message);
        call_wake_locked(call);
    }
}

static DREAM_THREAD_PROC(child_reap) {
    Child *child = arg;
    int32_t status;
#ifdef _WIN32
    DWORD code = 1;
    WaitForSingleObject(child->spawned.process, INFINITE);
    GetExitCodeProcess(child->spawned.process, &code);
    status = (int32_t)code;
#else
    int raw = 0;
    pid_t reaped;
    do {
        reaped = waitpid(child->spawned.pid, &raw, 0);
    } while (reaped < 0 && errno == EINTR);
    status = reaped < 0 ? -2 : WIFEXITED(raw) ? WEXITSTATUS(raw) : WIFSIGNALED(raw) ? -WTERMSIG(raw) : -2;
#endif
    dream_ptr waiters[64];
    int pending;
    dream_mutex_lock(&process_mu);
    child->exited = 1;
    child->status = status;
    do {
        pending = 0;
        ProcessCall *call = child->calls;
        while (call && pending < 64) {
            ProcessCall *next = call->next;
            if (call->kind == CALL_WAIT) {
                dream_ptr waiter = call_settle_locked(call, status, NULL, NULL);
                if (waiter) { waiters[pending++] = waiter; }
            }
            call = next;
        }
        dream_mutex_unlock(&process_mu);
        for (int i = 0; i < pending; ++i) { dream_complete_foreign(waiters[i], 0); }
        dream_mutex_lock(&process_mu);
    } while (pending == 64);
    child_drop_locked(child);
    dream_mutex_unlock(&process_mu);
    return 0;
}

#ifndef _WIN32
static void block_sigpipe(void) {
    sigset_t pipe_signal;
    sigemptyset(&pipe_signal);
    sigaddset(&pipe_signal, SIGPIPE);
    pthread_sigmask(SIG_BLOCK, &pipe_signal, NULL);
}

/* Waits until the pipe is ready or the call is woken. Returns 1 when ready. */
static int pipe_ready(ProcessCall *call, int fd, short events) {
    struct pollfd fds[2] = {{fd, events, 0}, {call->wake[0], POLLIN, 0}};
    for (;;) {
        int ready = poll(fds, 2, -1);
        if (ready < 0 && errno == EINTR) { continue; }
        if (ready < 0 || fds[1].revents) { return 0; }
        if (fds[0].revents) { return 1; }
    }
}
#endif

static DREAM_THREAD_PROC(call_io) {
    ProcessCall *call = arg;
    Child *child = call->child;
    int stream = call->stream;
    dream_mutex_lock(&child->stream_mu[stream]);
    dream_mutex_lock(&process_mu);
    dream_pipe pipe = child->spawned.pipes[stream];
    int cancelled = call->cancelled;
    dream_mutex_unlock(&process_mu);
    int64_t value = -1;
    const char *code = NULL;
    char message[160] = "";
    if (cancelled) {
        code = "ECANCELLED";
    } else if (pipe == DREAM_PIPE_NONE) {
        code = "ECLOSED";
        strcpy(message, stream == STREAM_STDIN ? "stdin closed" : "output closed");
    } else {
#ifdef _WIN32
        DWORD moved = 0;
        if (call->kind == CALL_READ) {
            DWORD delay = 1;
            for (;;) {
                dream_mutex_lock(&process_mu);
                cancelled = call->cancelled;
                dream_mutex_unlock(&process_mu);
                if (cancelled) { code = "ECANCELLED"; break; }
                DWORD available = 0;
                if (!PeekNamedPipe(pipe, NULL, 0, NULL, &available, NULL)) {
                    if (GetLastError() == ERROR_BROKEN_PIPE) { value = 0; } else { code = "EIO"; }
                    break;
                }
                if (available) {
                    DWORD size = available < (DWORD)call->count ? available : (DWORD)call->count;
                    if (ReadFile(pipe, call->bytes, size, &moved, NULL)) { value = moved; } else { code = "EIO"; }
                    break;
                }
                Sleep(delay);
                delay = delay < 16 ? delay * 2 : 16;
            }
        } else if (WriteFile(pipe, call->bytes, (DWORD)call->count, &moved, NULL)) {
            value = moved;
        } else {
            DWORD error = GetLastError();
            code = error == ERROR_OPERATION_ABORTED ? "ECANCELLED"
                : error == ERROR_BROKEN_PIPE || error == ERROR_NO_DATA ? "EPIPE" : "EIO";
        }
        if (code && !strcmp(code, "EIO")) { strcpy(message, "process pipe I/O failed"); }
#else
        block_sigpipe();
        short events = call->kind == CALL_READ ? POLLIN : POLLOUT;
        for (;;) {
            if (!pipe_ready(call, pipe, events)) { code = "ECANCELLED"; break; }
            ssize_t moved = call->kind == CALL_READ ? read(pipe, call->bytes, (size_t)call->count)
                                                     : write(pipe, call->bytes, (size_t)call->count);
            if (moved >= 0) { value = moved; break; }
            if (errno == EINTR || errno == EAGAIN || errno == EWOULDBLOCK) { continue; }
            code = errno == EPIPE ? "EPIPE" : "EIO";
            strncpy(message, strerror(errno), sizeof(message) - 1);
            break;
        }
#endif
    }
    dream_mutex_lock(&process_mu);
    if (call->cancelled) {
        if (call->kind == CALL_READ && value > 0) {
            leftover_prepend_locked(child, stream, call->bytes, (int32_t)value);
        }
        value = -1;
        code = call->code ? call->code : "ECANCELLED";
        snprintf(message, sizeof(message), "%s", call->message ? call->message : "process operation cancelled");
    }
    dream_ptr waiter = call_settle_locked(call, value, code, code == NULL ? NULL : message);
    dream_mutex_unlock(&process_mu);
    dream_mutex_unlock(&child->stream_mu[stream]);
    if (waiter) { dream_complete_foreign(waiter, 0); }
    call_drop(call);
    return 0;
}

static uintptr_t call_failed(int kind, const char *code, const char *message) {
    ProcessCall *call = call_new(kind, NULL, 0);
    if (!call) { return 0; }
    call->done = 1;
    call_set_error_locked(call, code, message);
    return (uintptr_t)call;
}

static int text_has_nul(dream_ptr text) {
    int32_t length = dream_str_len(text);
    const uint16_t *units = dream_str_units(text);
    for (int32_t i = 0; i < length; ++i) {
        if (!units[i]) { return 1; }
    }
    return 0;
}

static void free_texts(char **texts, int32_t count) {
    if (!texts) { return; }
    for (int32_t i = 0; i < count; ++i) { free(texts[i]); }
    free(texts);
}

static char **texts_utf8(dream_ptr array, int32_t *count, int *invalid) {
    int32_t length = array ? dream_i32(array)[0] : 0;
    dream_ptr *items = array ? (dream_ptr *)dream_array_bytes(array) : NULL;
    char **texts = calloc((size_t)length + 1, sizeof(char *));
    *count = length;
    if (!texts) { return NULL; }
    for (int32_t i = 0; i < length; ++i) {
        if (text_has_nul(items[i])) { *invalid = 1; }
        texts[i] = dream_str_utf8(items[i]);
        if (!texts[i]) {
            free_texts(texts, length);
            return NULL;
        }
    }
    return texts;
}

uintptr_t processSpawn(dream_ptr command, dream_ptr args, dream_ptr cwd, dream_ptr env, int32_t clear_env) {
    int invalid = text_has_nul(command) || text_has_nul(cwd);
    int32_t argc = 0, envc = 0;
    char **argv = texts_utf8(args, &argc, &invalid);
    char **envv = texts_utf8(env, &envc, &invalid);
    char *command_text = dream_str_utf8(command), *cwd_text = dream_str_utf8(cwd);
    Child *child = calloc(1, sizeof(*child));
    ProcessCall *call = call_new(CALL_SPAWN, NULL, 0);
    char message[512] = "";
    const char *code = NULL;
    if (!argv || !envv || !command_text || !cwd_text || !child || !call) {
        code = "EIO";
        strcpy(message, "out of memory");
    } else if (invalid || envc % 2) {
        code = "EINVAL";
        strcpy(message, "process text contains a NUL character");
    } else {
        int status = dream_process_spawn(command_text, argv, argc, cwd_text, envv, envc, clear_env != 0,
                                         &child->spawned, message, sizeof(message));
        if (status != DREAM_SPAWN_OK) { code = status == DREAM_SPAWN_INVALID ? "EINVAL" : "ESPAWN"; }
    }
    free_texts(argv, argc);
    free_texts(envv, envc);
    free(command_text);
    free(cwd_text);
    if (!call) {
        free(child);
        return 0;
    }
    call->done = 1;
    if (code) {
        free(child);
        call_set_error_locked(call, code, message);
        return (uintptr_t)call;
    }
    for (int i = 0; i < 3; ++i) { dream_mutex_init(&child->stream_mu[i]); }
    child->references = 2;
    dream_mutex_lock(&process_mu);
    child->id = next_child_id++;
    child->next = children;
    children = child;
    call->value = child->id;
    dream_mutex_unlock(&process_mu);
    dream_thread reaper;
    if (dream_thread_start(&reaper, child_reap, child) != 0) {
        /* Without a reaper the child could never be collected; refuse it instead. */
        dream_mutex_lock(&process_mu);
        child_kill_locked(child);
        child_unlink_locked(child);
        call->value = -1;
        call_set_error_locked(call, "ESPAWN", "cannot start the process supervisor thread");
        child_drop_locked(child);
        dream_mutex_unlock(&process_mu);
#ifndef _WIN32
        int reaped;
        do {
            reaped = waitpid(child->spawned.pid, NULL, 0);
        } while (reaped < 0 && errno == EINTR);
#endif
        dream_mutex_lock(&process_mu);
        child_drop_locked(child);
        dream_mutex_unlock(&process_mu);
        return (uintptr_t)call;
    }
    dream_thread_release(reaper);
    return (uintptr_t)call;
}

/* Starts a read or write on its own thread after the fast paths found nothing to do inline. */
static uintptr_t call_start_io(ProcessCall *call) {
#ifndef _WIN32
    if (dream_process_pipe(call->wake) != 0) {
        call->done = 1;
        call_set_error_locked(call, "EIO", "cannot create a process wake pipe");
        return (uintptr_t)call;
    }
#endif
    dream_mutex_lock(&process_mu);
    call->references = 2;
    call->next = call->child->calls;
    call->child->calls = call;
    dream_mutex_unlock(&process_mu);
    dream_thread thread;
    if (dream_thread_start(&thread, call_io, call) != 0) {
        call_settle(call, -1, "EIO", "cannot start a process I/O thread");
        call_drop(call);
        return (uintptr_t)call;
    }
#ifdef _WIN32
    dream_mutex_lock(&process_mu);
    call->thread = thread;
    dream_mutex_unlock(&process_mu);
#else
    dream_thread_release(thread);
#endif
    return (uintptr_t)call;
}

static Child *child_acquire_locked(int32_t id) {
    Child *child = child_find_locked(id);
    if (child) { ++child->references; }
    return child;
}

uintptr_t processRead(int32_t id, int32_t stream, int32_t max_bytes) {
    if ((stream != 0 && stream != 1) || max_bytes < 0 || max_bytes > PROCESS_READ_LIMIT) {
        return call_failed(CALL_READ, "EINVAL", "invalid process read");
    }
    int index = stream + 1;
    dream_mutex_lock(&process_mu);
    Child *child = child_acquire_locked(id);
    dream_mutex_unlock(&process_mu);
    if (!child) { return call_failed(CALL_READ, "ECLOSED", "child process is closed"); }
    ProcessCall *call = call_new(CALL_READ, child, index);
    uint8_t *bytes = malloc(max_bytes ? (size_t)max_bytes : 1);
    if (!call || !bytes) {
        free(bytes);
        free(call);
        dream_mutex_lock(&process_mu);
        child_drop_locked(child);
        dream_mutex_unlock(&process_mu);
        return 0;
    }
    call->bytes = bytes;
    call->count = max_bytes;
    dream_mutex_lock(&process_mu);
    int busy = 0;
    for (ProcessCall *other = child->calls; other; other = other->next) {
        busy |= other->kind == CALL_READ && other->stream == index;
    }
    int32_t pending = child->leftover_length[index];
    if (!max_bytes || (pending && !busy)) {
        int32_t size = pending < max_bytes ? pending : max_bytes;
        memcpy(bytes, child->leftover[index], (size_t)size);
        memmove(child->leftover[index], child->leftover[index] + size, (size_t)(pending - size));
        child->leftover_length[index] = pending - size;
        call->done = 1;
        call->value = size;
        dream_mutex_unlock(&process_mu);
        return (uintptr_t)call;
    }
    dream_pipe pipe = child->spawned.pipes[index];
    dream_mutex_unlock(&process_mu);
#ifndef _WIN32
    if (!busy && pipe >= 0) {
        ssize_t got = read(pipe, bytes, (size_t)max_bytes);
        if (got >= 0) {
            call->done = 1;
            call->value = got;
            return (uintptr_t)call;
        }
    }
#else
    (void)pipe;
#endif
    return call_start_io(call);
}

uintptr_t processWrite(int32_t id, dream_ptr data) {
    int32_t count = data ? dream_i32(data)[0] : 0;
    dream_mutex_lock(&process_mu);
    Child *child = child_acquire_locked(id);
    int closing = child && child->stdin_close;
    dream_mutex_unlock(&process_mu);
    if (!child) { return call_failed(CALL_WRITE, "ECLOSED", "child process is closed"); }
    ProcessCall *call = call_new(CALL_WRITE, child, STREAM_STDIN);
    uint8_t *bytes = malloc(count ? (size_t)count : 1);
    if (!call || !bytes) {
        free(bytes);
        free(call);
        dream_mutex_lock(&process_mu);
        child_drop_locked(child);
        dream_mutex_unlock(&process_mu);
        return 0;
    }
    if (count) { memcpy(bytes, dream_array_bytes(data), (size_t)count); }
    call->bytes = bytes;
    call->count = count;
    if (closing || !count) {
        call->done = 1;
        call->value = closing ? -1 : 0;
        if (closing) { call_set_error_locked(call, "ECLOSED", "stdin closed"); }
        return (uintptr_t)call;
    }
    return call_start_io(call);
}

uintptr_t processWait(int32_t id) {
    dream_mutex_lock(&process_mu);
    Child *child = child_acquire_locked(id);
    if (!child) {
        dream_mutex_unlock(&process_mu);
        return call_failed(CALL_WAIT, "ECLOSED", "child process is closed");
    }
    ProcessCall *call = call_new(CALL_WAIT, child, 0);
    if (!call) {
        child_drop_locked(child);
        dream_mutex_unlock(&process_mu);
        return 0;
    }
    if (child->exited) {
        call->done = 1;
        call->value = child->status;
    } else {
        call->next = child->calls;
        child->calls = call;
    }
    dream_mutex_unlock(&process_mu);
    return (uintptr_t)call;
}

int32_t processCallWaitAsync(dream_ptr future, uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    if (!call) {
        dream_async_complete(future, 0);
        return 0;
    }
    dream_mutex_lock(&process_mu);
    if (call->done) {
        dream_mutex_unlock(&process_mu);
        dream_async_complete(future, 0);
        return 0;
    }
    call->waiter = future;
    dream_mutex_unlock(&process_mu);
    return 1;
}

int64_t processCallValue(uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    if (!call) { return -1; }
    dream_mutex_lock(&process_mu);
    int64_t value = call->done && !call->code ? call->value : -1;
    if (call->done) { call->claimed = 1; }
    dream_mutex_unlock(&process_mu);
    return value;
}

dream_ptr processCallBytes(uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    int32_t count = 0;
    if (call) {
        dream_mutex_lock(&process_mu);
        if (call->done && !call->code && call->kind == CALL_READ && call->value > 0) {
            count = (int32_t)call->value;
        }
        if (call->done) { call->claimed = 1; }
        dream_mutex_unlock(&process_mu);
    }
    dream_ptr bytes = dream_array_new(count, 1);
    if (count) { memcpy(dream_array_bytes(bytes), call->bytes, (size_t)count); }
    return bytes;
}

dream_ptr processCallCode(uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    if (!call) { return dream_str_from_utf8("ELIMIT"); }
    dream_mutex_lock(&process_mu);
    const char *code = call->done && call->code ? call->code : "";
    dream_mutex_unlock(&process_mu);
    return dream_str_from_utf8(code);
}

dream_ptr processCallMessage(uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    if (!call) { return dream_str_from_utf8("process call capacity exhausted"); }
    dream_mutex_lock(&process_mu);
    char *message = strdup(call->message ? call->message : "");
    dream_mutex_unlock(&process_mu);
    dream_ptr text = dream_str_from_utf8(message ? message : "");
    free(message);
    return text;
}

static dream_ptr call_cancel_locked(ProcessCall *call) {
    if (call->done || call->cancelled) { return 0; }
    call->cancelled = 1;
    if (call->kind == CALL_WAIT) {
        return call_settle_locked(call, -1, "ECANCELLED", "process operation cancelled");
    }
    call_wake_locked(call);
    return 0;
}

void processCallCancel(uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    if (!call) { return; }
    dream_mutex_lock(&process_mu);
    dream_ptr waiter = call_cancel_locked(call);
    dream_mutex_unlock(&process_mu);
    if (waiter) { dream_complete_foreign(waiter, 0); }
}

void processCallRelease(uintptr_t handle) {
    ProcessCall *call = (ProcessCall *)handle;
    if (!call) { return; }
    processCallCancel(handle);
    call_drop(call);
}

void processCloseStdin(int32_t id) {
    dream_mutex_lock(&process_mu);
    Child *child = child_find_locked(id);
    if (child) {
        child->stdin_close = 1;
        stdin_close_locked(child);
    }
    dream_mutex_unlock(&process_mu);
}

void processKill(int32_t id) {
    dream_mutex_lock(&process_mu);
    Child *child = child_find_locked(id);
    if (child) {
        child_kill_locked(child);
        child_interrupt_locked(child, "ECLOSED", "child process closed");
    }
    dream_mutex_unlock(&process_mu);
}

void processRelease(int32_t id) {
    dream_mutex_lock(&process_mu);
    Child *child = child_find_locked(id);
    if (child) {
        child_unlink_locked(child);
        child_kill_locked(child);
        child_interrupt_locked(child, "ECLOSED", "child process closed");
        child_drop_locked(child);
    }
    dream_mutex_unlock(&process_mu);
}
