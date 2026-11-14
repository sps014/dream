#include "dream_socket.h"
#include "dream_host_support.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef _WIN32
#include <time.h>
#endif

/* One lock guards the socket table and every socket and call field. A call first tries its
 * operation without blocking; only when the socket is not ready does it move to a dedicated
 * thread that waits on the socket and a per-call wake. Components have a single thread, so every
 * call there runs to completion before its start function returns. */
#ifdef DREAM_COMPONENT
#define LOCK() ((void)0)
#define UNLOCK() ((void)0)
#else
#include "dream_thread.h"
static dream_mutex socket_mu = DREAM_MUTEX_INIT;
#define LOCK() dream_mutex_lock(&socket_mu)
#define UNLOCK() dream_mutex_unlock(&socket_mu)
#endif

enum { KIND_TCP, KIND_LISTENER, KIND_UDP };
enum { CALL_CONNECT, CALL_LISTEN, CALL_BIND, CALL_ACCEPT, CALL_READ, CALL_WRITE, CALL_SEND, CALL_RECEIVE, CALL_CONTROL };

/* Larger reads return short so a huge request does not preallocate its buffer. */
#define READ_CHUNK (1024 * 1024)
/* Holds the largest UDP payload; Windows fails a short receive instead of truncating it. */
#define DATAGRAM_BUFFER 65536
#define DATAGRAM_SEND_LIMIT 65507
#define SOCKET_LIMIT (1 << 20)

typedef struct SocketCall SocketCall;

typedef struct Socket {
    unsigned references;
    int32_t id;
    int kind, write_shut;
    dream_socket fd;
    /* What a cancelled read or receive already took, returned first by the next one. */
    uint8_t *leftover;
    int32_t leftover_length;
    int held;
    DreamSocketAddress held_peer;
    SocketCall *calls;
} Socket;

struct SocketCall {
    unsigned references;
    int kind, done, claimed, cancelled, resolved;
    Socket *socket;
    int64_t value;
    uint8_t *bytes;
    int32_t count;
    char *host;
    int32_t port, timeout_ms;
    DreamSocketAddress peer;
    dream_socket opened;
    const char *code;
    char *message;
    dream_ptr waiter;
    SocketCall *next;
#ifndef DREAM_COMPONENT
    DreamSocketWake wake;
    int has_wake;
#endif
};

typedef struct {
    int64_t value;
    dream_socket opened;
    DreamSocketError error;
} Outcome;

static Socket **table;
static int32_t *free_ids;
static int32_t table_capacity, free_count, next_id = 1;

/* Component calls finish before their start function returns, so nothing ever waits on one. */
static void waiter_complete(dream_ptr waiter) {
#ifdef DREAM_COMPONENT
    (void)waiter;
#else
    if (waiter) { dream_complete_foreign(waiter, 0); }
#endif
}

static int64_t now_ms(void) {
#ifdef _WIN32
    return (int64_t)GetTickCount64();
#else
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
#endif
}

static int is_open_kind(int kind) { return kind == CALL_CONNECT || kind == CALL_LISTEN || kind == CALL_BIND; }
static int is_read_side(int kind) { return kind == CALL_READ || kind == CALL_RECEIVE || kind == CALL_ACCEPT; }

static int32_t socket_register_locked(Socket *socket) {
    int32_t id;
    if (free_count) {
        id = free_ids[--free_count];
    } else {
        if (next_id >= SOCKET_LIMIT) { return 0; }
        if (next_id >= table_capacity) {
            int32_t capacity = table_capacity ? table_capacity * 2 : 64;
            Socket **grown = realloc(table, (size_t)capacity * sizeof(*grown));
            if (!grown) { return 0; }
            table = grown;
            int32_t *ids = realloc(free_ids, (size_t)capacity * sizeof(*ids));
            if (!ids) { return 0; }
            free_ids = ids;
            table_capacity = capacity;
        }
        id = next_id++;
    }
    table[id] = socket;
    socket->id = id;
    return id;
}

static Socket *socket_find_locked(int32_t id) { return id > 0 && id < next_id ? table[id] : NULL; }

static void socket_unregister_locked(Socket *socket) {
    if (socket->id <= 0) { return; }
    table[socket->id] = NULL;
    free_ids[free_count++] = socket->id;
    socket->id = 0;
}

static void socket_drop_locked(Socket *socket) {
    if (--socket->references) { return; }
    dream_socket_close(socket->fd);
    free(socket->leftover);
    free(socket);
}

static void call_wake_locked(SocketCall *call) {
#ifndef DREAM_COMPONENT
    if (call->has_wake) { dream_socket_wake_signal(&call->wake); }
#else
    (void)call;
#endif
}

static void call_set_error_locked(SocketCall *call, const char *code, const char *message) {
    call->code = code;
    free(call->message);
    call->message = strdup(message ? message : "");
}

/* Pending calls end promptly with `code`; their threads settle them once they let go. */
static void socket_interrupt_locked(Socket *socket, const char *code, const char *message) {
    for (SocketCall *call = socket->calls; call; call = call->next) {
        if (call->cancelled) { continue; }
        call->cancelled = 1;
        call_set_error_locked(call, code, message);
        call_wake_locked(call);
    }
}

static void socket_close_locked(Socket *socket) {
    socket_unregister_locked(socket);
    socket_interrupt_locked(socket, "ECLOSED", "socket is closed");
    socket_drop_locked(socket);
}

static Socket *socket_acquire_locked(int32_t id) {
    Socket *socket = socket_find_locked(id);
    if (socket) { ++socket->references; }
    return socket;
}

static void call_unlink_locked(SocketCall *call) {
    if (!call->socket) { return; }
    for (SocketCall **link = &call->socket->calls; *link; link = &(*link)->next) {
        if (*link == call) {
            *link = call->next;
            break;
        }
    }
    call->next = NULL;
}

static void leftover_prepend_locked(Socket *socket, const uint8_t *bytes, int32_t count) {
    if (count <= 0) { return; }
    int32_t old = socket->leftover_length;
    uint8_t *joined = malloc((size_t)old + (size_t)count);
    if (!joined) { return; }
    memcpy(joined, bytes, (size_t)count);
    if (old) { memcpy(joined + count, socket->leftover, (size_t)old); }
    free(socket->leftover);
    socket->leftover = joined;
    socket->leftover_length = old + count;
}

/* A datagram is kept whole: the next receive applies its own size limit and truncation flag. */
static void datagram_hold_locked(Socket *socket, const uint8_t *bytes, int32_t length, const DreamSocketAddress *peer) {
    if (socket->held) { return; }
    uint8_t *copy = malloc(length ? (size_t)length : 1);
    if (!copy) { return; }
    memcpy(copy, bytes, (size_t)length);
    free(socket->leftover);
    socket->leftover = copy;
    socket->leftover_length = length;
    socket->held_peer = *peer;
    socket->held = 1;
}

static void call_free_locked(SocketCall *call) {
    Socket *socket = call->socket;
    int unclaimed = call->done && !call->claimed && !call->code;
    if (unclaimed && call->kind == CALL_READ && call->value > 0) {
        leftover_prepend_locked(socket, call->bytes, (int32_t)call->value);
    }
    if (unclaimed && call->kind == CALL_RECEIVE && call->value >= 0) {
        int64_t kept = call->value < DATAGRAM_BUFFER ? call->value : DATAGRAM_BUFFER;
        datagram_hold_locked(socket, call->bytes, (int32_t)kept, &call->peer);
    }
    if (unclaimed && (is_open_kind(call->kind) || call->kind == CALL_ACCEPT)) {
        Socket *opened = socket_find_locked((int32_t)call->value);
        if (opened) { socket_close_locked(opened); }
    }
    if (socket) { socket_drop_locked(socket); }
    dream_socket_close(call->opened);
#ifndef DREAM_COMPONENT
    if (call->has_wake) { dream_socket_wake_close(&call->wake); }
#endif
    free(call->bytes);
    free(call->host);
    free(call->message);
    free(call);
}

static void call_drop(SocketCall *call) {
    LOCK();
    if (!--call->references) { call_free_locked(call); }
    UNLOCK();
}

static SocketCall *call_new(int kind, Socket *socket) {
    SocketCall *call = calloc(1, sizeof(*call));
    if (!call) { return NULL; }
    call->references = 1;
    call->kind = kind;
    call->socket = socket;
    call->value = -1;
    call->opened = DREAM_SOCKET_NONE;
    return call;
}

/* Settles the call and returns its waiter, which the caller completes outside the lock. */
static dream_ptr call_settle_locked(SocketCall *call, int64_t value, const char *code, const char *message) {
    if (call->done) { return 0; }
    call->done = 1;
    call->value = value;
    if (code) { call_set_error_locked(call, code, message); }
    call_unlink_locked(call);
    dream_ptr waiter = call->waiter;
    call->waiter = 0;
    return waiter;
}

static uintptr_t call_failed(int kind, Socket *socket, const char *code, const char *message) {
    SocketCall *call = call_new(kind, socket);
    if (!call) {
        if (socket) {
            LOCK();
            socket_drop_locked(socket);
            UNLOCK();
        }
        return 0;
    }
    call->done = 1;
    call_set_error_locked(call, code, message);
    return (uintptr_t)call;
}

static int call_cancelled(SocketCall *call) {
    LOCK();
    int cancelled = call->cancelled;
    UNLOCK();
    return cancelled;
}

static int remaining_ms(int64_t deadline) {
    if (deadline < 0) { return -1; }
    int64_t left = deadline - now_ms();
    return left > 0 ? (int)left : 0;
}

static DreamSocketWake *call_wake(SocketCall *call) {
#ifndef DREAM_COMPONENT
    return call->has_wake ? &call->wake : NULL;
#else
    (void)call;
    return NULL;
#endif
}

/* Waits for the socket; on failure fills `out` and returns 0. */
static int call_wait(SocketCall *call, dream_socket fd, int write, int64_t deadline, Outcome *out) {
    int left = remaining_ms(deadline);
    int status = left == 0 ? DREAM_WAIT_TIMEOUT : dream_socket_wait(fd, write, call_wake(call), left);
    if (status == DREAM_WAIT_READY) { return 1; }
    if (status == DREAM_WAIT_TIMEOUT) {
        dream_socket_fail(&out->error, "ETIMEOUT", "socket operation deadline exceeded");
    } else if (status == DREAM_WAIT_WOKEN) {
        dream_socket_fail(&out->error, "ECANCELLED", "socket operation cancelled");
    } else {
        dream_socket_fail(&out->error, "EIO", "cannot wait for socket");
    }
    return 0;
}

static void run_open(SocketCall *call, Outcome *out) {
    int64_t deadline = call->timeout_ms > 0 ? now_ms() + call->timeout_ms : -1;
    struct addrinfo *addresses = NULL;
    if (dream_socket_resolve(call->host, call->port, call->kind == CALL_BIND, 0, AF_UNSPEC, &addresses, &out->error)) {
        return;
    }
    for (struct addrinfo *address = addresses; address; address = address->ai_next) {
        if (call_cancelled(call)) {
            dream_socket_fail(&out->error, "ECANCELLED", "socket operation cancelled");
            break;
        }
        dream_socket fd = dream_socket_open(address, &out->error);
        if (fd == DREAM_SOCKET_NONE) { continue; }
        int status;
        if (call->kind == CALL_CONNECT) {
            status = dream_socket_connect_start(fd, address, &out->error);
            if (status == DREAM_IO_BLOCKED) {
                status = call_wait(call, fd, 1, deadline, out) ? dream_socket_connect_finish(fd, &out->error)
                                                               : DREAM_IO_FAILED;
            }
        } else {
            status = dream_socket_listen(fd, address, call->kind == CALL_LISTEN, &out->error);
        }
        if (status == DREAM_IO_DONE) {
            out->error.code = NULL;
            out->opened = fd;
            break;
        }
        dream_socket_close(fd);
        if (!strcmp(out->error.code, "ETIMEOUT") || !strcmp(out->error.code, "ECANCELLED")) { break; }
    }
    freeaddrinfo(addresses);
}

/* Tries the operation once without blocking. */
static int call_attempt(SocketCall *call, Outcome *out) {
    dream_socket fd = call->socket->fd;
    switch (call->kind) {
    case CALL_ACCEPT:
        return dream_socket_accept(fd, &out->opened, &out->error);
    case CALL_READ:
        return dream_socket_recv(fd, call->bytes, call->count, &out->value, &out->error);
    case CALL_WRITE:
        return dream_socket_send(fd, call->bytes, call->count, &out->value, &out->error);
    case CALL_SEND:
        return dream_socket_sendto(fd, call->bytes, call->count, &call->peer, &out->value, &out->error);
    default:
        return dream_socket_recvfrom(fd, call->bytes, DATAGRAM_BUFFER, &out->value, &call->peer, &out->error);
    }
}

static int resolve_target(SocketCall *call, int numeric, DreamSocketError *error) {
    struct addrinfo *addresses = NULL;
    int status = dream_socket_resolve(call->host, call->port, 1, numeric, dream_socket_family(call->socket->fd),
                                      &addresses, error);
    if (status) { return status; }
    memcpy(&call->peer.storage, addresses->ai_addr, (size_t)addresses->ai_addrlen);
    call->peer.length = (socklen_t)addresses->ai_addrlen;
    call->resolved = 1;
    freeaddrinfo(addresses);
    return 0;
}

static void run_io(SocketCall *call, Outcome *out) {
    int64_t deadline = call->timeout_ms > 0 ? now_ms() + call->timeout_ms : -1;
    if (call->kind == CALL_SEND && !call->resolved && resolve_target(call, 0, &out->error)) { return; }
    int write = call->kind == CALL_WRITE || call->kind == CALL_SEND;
    for (;;) {
        if (call_cancelled(call)) {
            dream_socket_fail(&out->error, "ECANCELLED", "socket operation cancelled");
            return;
        }
        if (call_attempt(call, out) != DREAM_IO_BLOCKED) { return; }
        if (!call_wait(call, call->socket->fd, write, deadline, out)) { return; }
    }
}

static void call_complete(SocketCall *call, Outcome *out) {
    const char *code = out->error.code;
    char message[160];
    snprintf(message, sizeof(message), "%s", out->error.message);
    int64_t value = code ? -1 : out->value;
    LOCK();
    Socket *socket = call->socket;
    if (call->cancelled && !call->done) {
        if (!code && call->kind == CALL_READ && value > 0) {
            leftover_prepend_locked(socket, call->bytes, (int32_t)value);
        }
        if (!code && call->kind == CALL_RECEIVE && value >= 0) {
            datagram_hold_locked(socket, call->bytes, (int32_t)(value < DATAGRAM_BUFFER ? value : DATAGRAM_BUFFER),
                                 &call->peer);
        }
        value = -1;
        code = call->code ? call->code : "ECANCELLED";
        snprintf(message, sizeof(message), "%s", call->message ? call->message : "socket operation cancelled");
    }
    if (out->opened != DREAM_SOCKET_NONE) {
        Socket *opened = code || call->done ? NULL : calloc(1, sizeof(*opened));
        if (opened) {
            opened->references = 1;
            opened->fd = out->opened;
            opened->kind = call->kind == CALL_BIND ? KIND_UDP : call->kind == CALL_LISTEN ? KIND_LISTENER : KIND_TCP;
            value = socket_register_locked(opened);
            if (!value) {
                free(opened);
                opened = NULL;
                code = "ELIMIT";
                snprintf(message, sizeof(message), "socket capacity exhausted");
            }
        }
        if (!opened) { call->opened = out->opened; }
    }
    dream_ptr waiter = call_settle_locked(call, value, code, code ? message : NULL);
    UNLOCK();
    waiter_complete(waiter);
}

static void call_run(SocketCall *call) {
    Outcome out;
    memset(&out, 0, sizeof(out));
    out.opened = DREAM_SOCKET_NONE;
    if (is_open_kind(call->kind)) {
        run_open(call, &out);
    } else {
        run_io(call, &out);
    }
    call_complete(call, &out);
}

#ifndef DREAM_COMPONENT
static DREAM_THREAD_PROC(call_thread) {
    SocketCall *call = arg;
    call_run(call);
    call_drop(call);
    return 0;
}
#endif

/* Runs the call: inline on components, otherwise on its own thread. */
static uintptr_t call_start(SocketCall *call) {
#ifdef DREAM_COMPONENT
    call_run(call);
#else
    if (dream_socket_wake_open(&call->wake) != 0) {
        Outcome out;
        memset(&out, 0, sizeof(out));
        out.opened = DREAM_SOCKET_NONE;
        dream_socket_fail(&out.error, "EIO", "cannot create a socket wake");
        call_complete(call, &out);
        return (uintptr_t)call;
    }
    LOCK();
    call->has_wake = 1;
    call->references = 2;
    UNLOCK();
    dream_thread thread;
    if (dream_thread_start(&thread, call_thread, call) != 0) {
        Outcome out;
        memset(&out, 0, sizeof(out));
        out.opened = DREAM_SOCKET_NONE;
        dream_socket_fail(&out.error, "EIO", "cannot start a socket thread");
        call_complete(call, &out);
        call_drop(call);
        return (uintptr_t)call;
    }
    dream_thread_release(thread);
#endif
    return (uintptr_t)call;
}

static char *text_utf8(dream_ptr text) {
    int32_t length = dream_str_len(text);
    const uint16_t *units = dream_str_units(text);
    for (int32_t i = 0; i < length; ++i) {
        if (!units[i]) { return NULL; }
    }
    return dream_str_utf8(text);
}

static uintptr_t open_call(int kind, dream_ptr host, int32_t port, int32_t timeout_ms) {
    DreamSocketError error;
    if (dream_socket_startup(&error) != 0) { return call_failed(kind, NULL, error.code, error.message); }
    if (port < 0 || port > 65535 || timeout_ms < 0) {
        return call_failed(kind, NULL, "EINVAL", "invalid socket port or timeout");
    }
    char *text = text_utf8(host);
    if (!text || !text[0]) {
        free(text);
        return call_failed(kind, NULL, "EINVAL", "invalid socket host");
    }
    SocketCall *call = call_new(kind, NULL);
    if (!call) {
        free(text);
        return 0;
    }
    call->host = text;
    call->port = port;
    call->timeout_ms = timeout_ms;
    return call_start(call);
}

uintptr_t netSocketConnect(dream_ptr host, int32_t port, int32_t timeout_ms) {
    return open_call(CALL_CONNECT, host, port, timeout_ms);
}

uintptr_t netSocketListen(dream_ptr host, int32_t port) { return open_call(CALL_LISTEN, host, port, 0); }

uintptr_t netSocketBind(dream_ptr host, int32_t port) { return open_call(CALL_BIND, host, port, 0); }

/* Creates a call on an open socket of `expected` kind and links it as pending, unless another
 * call already holds that side of the socket. Returns NULL with `*handle` set on failure. */
static SocketCall *io_call(int kind, int32_t id, int expected, int32_t timeout_ms, uintptr_t *handle) {
    *handle = 0;
    if (timeout_ms < 0) {
        *handle = call_failed(kind, NULL, "EINVAL", "invalid socket timeout");
        return NULL;
    }
    LOCK();
    Socket *socket = socket_acquire_locked(id);
    UNLOCK();
    if (!socket) {
        *handle = call_failed(kind, NULL, "ECLOSED", "socket is closed");
        return NULL;
    }
    if (socket->kind != expected) {
        *handle = call_failed(kind, socket, "EINVAL", "operation does not match socket kind");
        return NULL;
    }
    SocketCall *call = call_new(kind, socket);
    if (!call) {
        LOCK();
        socket_drop_locked(socket);
        UNLOCK();
        return NULL;
    }
    call->timeout_ms = timeout_ms;
    LOCK();
    int busy = 0;
    for (SocketCall *other = socket->calls; other; other = other->next) {
        busy |= kind == CALL_WRITE ? other->kind == CALL_WRITE : is_read_side(kind) && is_read_side(other->kind);
    }
    if (busy) {
        call->done = 1;
        call_set_error_locked(call, "EBUSY", kind == CALL_WRITE ? "socket already has a pending write"
                                                                : "socket already has a pending read");
        UNLOCK();
        *handle = (uintptr_t)call;
        return NULL;
    }
    call->next = socket->calls;
    socket->calls = call;
    UNLOCK();
    return call;
}

#ifndef DREAM_COMPONENT
/* Settles a call that finished without blocking; returns 0 when it must wait instead. */
static int io_try(SocketCall *call) {
    Outcome out;
    memset(&out, 0, sizeof(out));
    out.opened = DREAM_SOCKET_NONE;
    if (call_attempt(call, &out) == DREAM_IO_BLOCKED) { return 0; }
    call_complete(call, &out);
    return 1;
}
#endif

static uintptr_t io_start(SocketCall *call) {
#ifndef DREAM_COMPONENT
    if (io_try(call)) { return (uintptr_t)call; }
#endif
    return call_start(call);
}

uintptr_t netSocketAccept(int32_t id, int32_t timeout_ms) {
    uintptr_t handle;
    SocketCall *call = io_call(CALL_ACCEPT, id, KIND_LISTENER, timeout_ms, &handle);
    return call ? io_start(call) : handle;
}

uintptr_t netSocketRead(int32_t id, int32_t max_bytes, int32_t timeout_ms) {
    if (max_bytes < 0) { return call_failed(CALL_READ, NULL, "EINVAL", "invalid socket read size"); }
    uintptr_t handle;
    SocketCall *call = io_call(CALL_READ, id, KIND_TCP, timeout_ms, &handle);
    if (!call) { return handle; }
    int32_t count = max_bytes < READ_CHUNK ? max_bytes : READ_CHUNK;
    call->bytes = malloc(count ? (size_t)count : 1);
    call->count = count;
    LOCK();
    Socket *socket = call->socket;
    if (!call->bytes || !count || socket->leftover_length) {
        int32_t size = !call->bytes ? 0 : socket->leftover_length < count ? socket->leftover_length : count;
        if (size) {
            memcpy(call->bytes, socket->leftover, (size_t)size);
            memmove(socket->leftover, socket->leftover + size, (size_t)(socket->leftover_length - size));
            socket->leftover_length -= size;
        }
        call_settle_locked(call, call->bytes ? size : -1, call->bytes ? NULL : "EIO", "out of memory");
        UNLOCK();
        return (uintptr_t)call;
    }
    UNLOCK();
    return io_start(call);
}

uintptr_t netSocketWrite(int32_t id, dream_ptr data) {
    uintptr_t handle;
    SocketCall *call = io_call(CALL_WRITE, id, KIND_TCP, 0, &handle);
    if (!call) { return handle; }
    int32_t count = data ? dream_i32(data)[0] : 0;
    call->bytes = malloc(count ? (size_t)count : 1);
    call->count = count;
    if (call->bytes && count) { memcpy(call->bytes, dream_array_bytes(data), (size_t)count); }
    LOCK();
    const char *code = !call->bytes ? "EIO" : call->socket->write_shut ? "ECLOSED" : NULL;
    if (code || !count) {
        call_settle_locked(call, code ? -1 : 0, code,
                           call->bytes ? "socket write side is shut down" : "out of memory");
        UNLOCK();
        return (uintptr_t)call;
    }
    UNLOCK();
    return io_start(call);
}

uintptr_t netSocketSend(int32_t id, dream_ptr host, int32_t port, dream_ptr data, int32_t timeout_ms) {
    int32_t count = data ? dream_i32(data)[0] : 0;
    if (port < 0 || port > 65535 || count > DATAGRAM_SEND_LIMIT) {
        return call_failed(CALL_SEND, NULL, "EINVAL", "invalid datagram endpoint or size");
    }
    char *text = text_utf8(host);
    if (!text || !text[0]) {
        free(text);
        return call_failed(CALL_SEND, NULL, "EINVAL", "invalid socket host");
    }
    uintptr_t handle;
    SocketCall *call = io_call(CALL_SEND, id, KIND_UDP, timeout_ms, &handle);
    if (!call) {
        free(text);
        return handle;
    }
    call->host = text;
    call->port = port;
    call->bytes = malloc(count ? (size_t)count : 1);
    call->count = count;
    if (!call->bytes) {
        LOCK();
        call_settle_locked(call, -1, "EIO", "out of memory");
        UNLOCK();
        return (uintptr_t)call;
    }
    if (count) { memcpy(call->bytes, dream_array_bytes(data), (size_t)count); }
    DreamSocketError error;
    int status = resolve_target(call, 1, &error);
    if (status < 0) {
        LOCK();
        call_settle_locked(call, -1, error.code, error.message);
        UNLOCK();
        return (uintptr_t)call;
    }
    return status == 0 ? io_start(call) : call_start(call);
}

uintptr_t netSocketReceive(int32_t id, int32_t max_bytes, int32_t timeout_ms) {
    if (max_bytes < 0 || max_bytes > 65535) {
        return call_failed(CALL_RECEIVE, NULL, "EINVAL", "invalid datagram read size");
    }
    uintptr_t handle;
    SocketCall *call = io_call(CALL_RECEIVE, id, KIND_UDP, timeout_ms, &handle);
    if (!call) { return handle; }
    call->bytes = malloc(DATAGRAM_BUFFER);
    call->count = max_bytes;
    LOCK();
    Socket *socket = call->socket;
    if (!call->bytes || socket->held) {
        int64_t length = -1;
        if (call->bytes) {
            length = socket->leftover_length;
            memcpy(call->bytes, socket->leftover, (size_t)length);
            call->peer = socket->held_peer;
            socket->held = 0;
            socket->leftover_length = 0;
        }
        call_settle_locked(call, length, call->bytes ? NULL : "EIO", "out of memory");
        UNLOCK();
        return (uintptr_t)call;
    }
    UNLOCK();
    return io_start(call);
}

uintptr_t netSocketShutdown(int32_t id) {
    LOCK();
    Socket *socket = socket_acquire_locked(id);
    const char *code = NULL, *message = NULL;
    if (!socket) {
        code = "ECLOSED";
        message = "socket is closed";
    } else if (socket->kind != KIND_TCP) {
        code = "EINVAL";
        message = "operation does not match socket kind";
    } else {
        for (SocketCall *other = socket->calls; other; other = other->next) {
            if (other->kind == CALL_WRITE) {
                code = "EBUSY";
                message = "write is pending";
            }
        }
    }
    DreamSocketError error;
    if (!code && !socket->write_shut) {
        if (dream_socket_shutdown_write(socket->fd, &error) == 0) {
            socket->write_shut = 1;
        } else {
            code = error.code;
            message = error.message;
        }
    }
    UNLOCK();
    if (code) { return call_failed(CALL_CONTROL, socket, code, message); }
    SocketCall *call = call_new(CALL_CONTROL, socket);
    if (!call) {
        LOCK();
        socket_drop_locked(socket);
        UNLOCK();
        return 0;
    }
    call->done = 1;
    call->value = 0;
    return (uintptr_t)call;
}

uintptr_t netSocketOption(int32_t id, int32_t option, int32_t value) {
    LOCK();
    Socket *socket = socket_acquire_locked(id);
    UNLOCK();
    if (!socket) { return call_failed(CALL_CONTROL, NULL, "ECLOSED", "socket is closed"); }
    int expected = option == 0 ? KIND_TCP : KIND_UDP;
    if (socket->kind != expected || option < 0 || option > 2 || (option == 2 && (value < 1 || value > 255))) {
        return call_failed(CALL_CONTROL, socket, "EINVAL", "unsupported socket option");
    }
    DreamSocketError error;
    if (dream_socket_set_option(socket->fd, option, value, &error) != 0) {
        return call_failed(CALL_CONTROL, socket, error.code, error.message);
    }
    SocketCall *call = call_new(CALL_CONTROL, socket);
    if (!call) {
        LOCK();
        socket_drop_locked(socket);
        UNLOCK();
        return 0;
    }
    call->done = 1;
    call->value = 0;
    return (uintptr_t)call;
}

#ifdef DREAM_COMPONENT
void netSocketCallWait(dream_ptr future, uintptr_t handle) {
    (void)handle;
    dream_async_complete(future, 0);
}
#else
int32_t netSocketCallWaitAsync(dream_ptr future, uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    LOCK();
    if (!call || call->done) {
        UNLOCK();
        dream_async_complete(future, 0);
        return 0;
    }
    /* A cancelled wait is awaited again; the scheduler still counts the first future, so it must
     * complete rather than be forgotten. */
    dream_ptr superseded = call->waiter;
    call->waiter = future;
    UNLOCK();
    waiter_complete(superseded);
    return 1;
}
#endif

int64_t netSocketCallValue(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    if (!call) { return -1; }
    LOCK();
    int64_t value = call->done && !call->code ? call->value : -1;
    if (call->done) { call->claimed = 1; }
    UNLOCK();
    return value;
}

dream_ptr netSocketCallBytes(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    int32_t count = 0;
    if (call) {
        LOCK();
        if (call->done && !call->code && (call->kind == CALL_READ || call->kind == CALL_RECEIVE) && call->value > 0) {
            count = (int32_t)(call->value < call->count ? call->value : call->count);
        }
        if (call->done) { call->claimed = 1; }
        UNLOCK();
    }
    dream_ptr bytes = dream_array_new(count, 1);
    if (count) { memcpy(dream_array_bytes(bytes), call->bytes, (size_t)count); }
    return bytes;
}

dream_ptr netSocketCallHost(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    char host[64] = "";
    if (call && call->done && !call->code && call->kind == CALL_RECEIVE) {
        dream_socket_address_text(&call->peer, host, sizeof(host));
    }
    return dream_str_from_utf8(host);
}

int32_t netSocketCallPort(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    char host[64];
    if (!call || !call->done || call->code || call->kind != CALL_RECEIVE) { return 0; }
    int port = dream_socket_address_text(&call->peer, host, sizeof(host));
    return port < 0 ? 0 : port;
}

dream_ptr netSocketCallCode(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    if (!call) { return dream_str_from_utf8("ELIMIT"); }
    LOCK();
    const char *code = call->done && call->code ? call->code : "";
    UNLOCK();
    return dream_str_from_utf8(code);
}

dream_ptr netSocketCallMessage(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    if (!call) { return dream_str_from_utf8("socket call capacity exhausted"); }
    LOCK();
    char *message = strdup(call->message ? call->message : "");
    UNLOCK();
    dream_ptr text = dream_str_from_utf8(message ? message : "");
    free(message);
    return text;
}

void netSocketCallCancel(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    if (!call) { return; }
    dream_ptr waiter = 0;
    LOCK();
    if (!call->done && !call->cancelled) {
        call->cancelled = 1;
        call_wake_locked(call);
        /* A name lookup cannot be interrupted, so an open settles now and its thread closes
         * whatever socket it still produces. */
        if (is_open_kind(call->kind)) {
            waiter = call_settle_locked(call, -1, "ECANCELLED", "socket operation cancelled");
        }
    }
    UNLOCK();
    waiter_complete(waiter);
}

void netSocketCallRelease(uintptr_t handle) {
    SocketCall *call = (SocketCall *)handle;
    if (!call) { return; }
    netSocketCallCancel(handle);
    call_drop(call);
}

dream_ptr netSocketAddress(int32_t id, int32_t peer) {
    char host[64] = "";
    LOCK();
    Socket *socket = socket_find_locked(id);
    if (socket) { dream_socket_endpoint(socket->fd, peer, host, sizeof(host)); }
    UNLOCK();
    return dream_str_from_utf8(host);
}

int32_t netSocketPort(int32_t id, int32_t peer) {
    char host[64];
    int port = -1;
    LOCK();
    Socket *socket = socket_find_locked(id);
    if (socket) { port = dream_socket_endpoint(socket->fd, peer, host, sizeof(host)); }
    UNLOCK();
    return port < 0 ? 0 : port;
}

void netSocketClose(int32_t id) {
    LOCK();
    Socket *socket = socket_find_locked(id);
    if (socket) { socket_close_locked(socket); }
    UNLOCK();
}
