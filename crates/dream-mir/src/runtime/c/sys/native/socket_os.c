#if defined(__linux__) && !defined(_GNU_SOURCE)
#define _GNU_SOURCE
#endif

#include "dream_socket.h"

#include <stdio.h>
#include <string.h>

#ifdef _WIN32
#define SOCKET_WOULD_BLOCK(code) ((code) == WSAEWOULDBLOCK)
#define SOCKET_INTERRUPTED(code) ((code) == WSAEINTR)
#define SOCKET_IN_PROGRESS(code) ((code) == WSAEWOULDBLOCK || (code) == WSAEINPROGRESS)
typedef WSAPOLLFD dream_pollfd;
#define dream_poll WSAPoll
typedef int socket_size;
static int socket_errno(void) { return WSAGetLastError(); }
#else
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <poll.h>
#include <unistd.h>
#define SOCKET_WOULD_BLOCK(code) ((code) == EAGAIN || (code) == EWOULDBLOCK)
#define SOCKET_INTERRUPTED(code) ((code) == EINTR)
#define SOCKET_IN_PROGRESS(code) ((code) == EINPROGRESS)
typedef struct pollfd dream_pollfd;
#define dream_poll poll
typedef size_t socket_size;
static int socket_errno(void) { return errno; }
#endif

/* Linux reports a broken connection to send as SIGPIPE unless asked not to; Apple sockets carry
 * SO_NOSIGPIPE instead. WASI has no signals, and its UDP sendto rejects the flag. */
#if defined(MSG_NOSIGNAL) && !defined(__wasi__)
#define SEND_FLAGS MSG_NOSIGNAL
#else
#define SEND_FLAGS 0
#endif

#ifndef AI_NUMERICSERV
#define AI_NUMERICSERV 0
#endif

enum { OPTION_NODELAY, OPTION_BROADCAST, OPTION_TTL };

void dream_socket_fail(DreamSocketError *error, const char *code, const char *message) {
    error->code = code;
    snprintf(error->message, sizeof(error->message), "%s", message);
}

static void fail_system(DreamSocketError *error, int code, const char *action) {
    error->code = "EIO";
#ifdef _WIN32
    char text[128] = "";
    FormatMessageA(FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS, NULL, (DWORD)code, 0, text,
                   sizeof(text), NULL);
    size_t length = strlen(text);
    while (length && (text[length - 1] == '\n' || text[length - 1] == '\r' || text[length - 1] == '.')) {
        text[--length] = 0;
    }
    snprintf(error->message, sizeof(error->message), "%s: %s", action, length ? text : "socket error");
#else
    snprintf(error->message, sizeof(error->message), "%s: %s", action, strerror(code));
#endif
}

int dream_socket_startup(DreamSocketError *error) {
#ifdef _WIN32
    static volatile LONG started;
    if (started) { return 0; }
    WSADATA data;
    int code = WSAStartup(MAKEWORD(2, 2), &data);
    if (code) {
        fail_system(error, code, "cannot start Winsock");
        return -1;
    }
    InterlockedExchange(&started, 1);
#else
    (void)error;
#endif
    return 0;
}

void dream_socket_close(dream_socket socket) {
    if (socket == DREAM_SOCKET_NONE) { return; }
#ifdef _WIN32
    closesocket(socket);
#else
    close(socket);
#endif
}

int dream_socket_resolve(const char *host, int port, int datagram, int numeric, int family,
                         struct addrinfo **addresses, DreamSocketError *error) {
    char service[8];
    snprintf(service, sizeof(service), "%d", port);
    struct addrinfo hints;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = family;
    hints.ai_socktype = datagram ? SOCK_DGRAM : SOCK_STREAM;
    hints.ai_flags = AI_NUMERICSERV | (numeric ? AI_NUMERICHOST : 0);
    int code = getaddrinfo(host, service, &hints, addresses);
    if (code == 0 && *addresses) { return 0; }
    if (code == 0) { freeaddrinfo(*addresses); }
    *addresses = NULL;
    if (numeric && code == EAI_NONAME) { return 1; }
    snprintf(error->message, sizeof(error->message), "cannot resolve %s: %s", host,
#ifdef _WIN32
             "name lookup failed"
#else
             gai_strerror(code)
#endif
    );
    error->code = "EIO";
    return -1;
}

static int set_nonblocking(dream_socket socket) {
#ifdef _WIN32
    u_long enabled = 1;
    return ioctlsocket(socket, FIONBIO, &enabled) == 0 ? 0 : -1;
#else
    int flags = fcntl(socket, F_GETFL, 0);
    return flags >= 0 && fcntl(socket, F_SETFL, flags | O_NONBLOCK) == 0 ? 0 : -1;
#endif
}

/* Child processes must not inherit sockets; Linux sets that atomically at creation. */
static void prepare(dream_socket socket) {
    (void)socket;
#ifdef _WIN32
    SetHandleInformation((HANDLE)socket, HANDLE_FLAG_INHERIT, 0);
#elif !defined(SOCK_CLOEXEC) && !defined(__wasi__)
    fcntl(socket, F_SETFD, FD_CLOEXEC);
#endif
#ifdef SO_NOSIGPIPE
    int enabled = 1;
    setsockopt(socket, SOL_SOCKET, SO_NOSIGPIPE, &enabled, sizeof(enabled));
#endif
}

dream_socket dream_socket_open(const struct addrinfo *address, DreamSocketError *error) {
#ifdef _WIN32
    SOCKET opened = WSASocketW(address->ai_family, address->ai_socktype, address->ai_protocol, NULL, 0,
                               WSA_FLAG_OVERLAPPED | WSA_FLAG_NO_HANDLE_INHERIT);
#elif defined(SOCK_CLOEXEC)
    int opened = socket(address->ai_family, address->ai_socktype | SOCK_CLOEXEC, address->ai_protocol);
#else
    int opened = socket(address->ai_family, address->ai_socktype, address->ai_protocol);
#endif
    if (opened == DREAM_SOCKET_NONE) {
        fail_system(error, socket_errno(), "cannot create socket");
        return DREAM_SOCKET_NONE;
    }
    prepare(opened);
    if (set_nonblocking(opened) != 0) {
        fail_system(error, socket_errno(), "cannot configure socket");
        dream_socket_close(opened);
        return DREAM_SOCKET_NONE;
    }
    return opened;
}

int dream_socket_connect_start(dream_socket socket, const struct addrinfo *address, DreamSocketError *error) {
    if (connect(socket, address->ai_addr, (socklen_t)address->ai_addrlen) == 0) { return DREAM_IO_DONE; }
    int code = socket_errno();
    if (SOCKET_IN_PROGRESS(code) || SOCKET_INTERRUPTED(code)) { return DREAM_IO_BLOCKED; }
    fail_system(error, code, "cannot connect");
    return DREAM_IO_FAILED;
}

int dream_socket_connect_finish(dream_socket socket, DreamSocketError *error) {
    int code = 0;
    socklen_t length = sizeof(code);
    if (getsockopt(socket, SOL_SOCKET, SO_ERROR, (char *)&code, &length) != 0) { code = socket_errno(); }
    if (!code) { return DREAM_IO_DONE; }
    fail_system(error, code, "cannot connect");
    return DREAM_IO_FAILED;
}

int dream_socket_listen(dream_socket socket, const struct addrinfo *address, int stream, DreamSocketError *error) {
#if !defined(_WIN32) && !defined(__wasi__)
    /* A restarted server may rebind a port whose old connections are still in TIME_WAIT. */
    int enabled = 1;
    if (stream) { setsockopt(socket, SOL_SOCKET, SO_REUSEADDR, &enabled, sizeof(enabled)); }
#endif
    if (bind(socket, address->ai_addr, (socklen_t)address->ai_addrlen) != 0) {
        fail_system(error, socket_errno(), "cannot bind");
        return DREAM_IO_FAILED;
    }
    if (stream && listen(socket, 128) != 0) {
        fail_system(error, socket_errno(), "cannot listen");
        return DREAM_IO_FAILED;
    }
    return DREAM_IO_DONE;
}

static int io_failed(int code, const char *action, DreamSocketError *error) {
    if (SOCKET_WOULD_BLOCK(code) || SOCKET_INTERRUPTED(code)) { return DREAM_IO_BLOCKED; }
    fail_system(error, code, action);
    return DREAM_IO_FAILED;
}

int dream_socket_accept(dream_socket socket, dream_socket *accepted, DreamSocketError *error) {
#if defined(__linux__)
    dream_socket opened = accept4(socket, NULL, NULL, SOCK_CLOEXEC | SOCK_NONBLOCK);
#else
    dream_socket opened = accept(socket, NULL, NULL);
#endif
    if (opened == DREAM_SOCKET_NONE) {
        int code = socket_errno();
#ifndef _WIN32
        /* The client gave up before we accepted; wait for the next one. */
        if (code == ECONNABORTED) { return DREAM_IO_BLOCKED; }
#endif
        return io_failed(code, "cannot accept", error);
    }
#if !defined(__linux__)
    prepare(opened);
    if (set_nonblocking(opened) != 0) {
        fail_system(error, socket_errno(), "cannot configure socket");
        dream_socket_close(opened);
        return DREAM_IO_FAILED;
    }
#endif
    *accepted = opened;
    return DREAM_IO_DONE;
}

int dream_socket_recv(dream_socket socket, uint8_t *bytes, int32_t count, int64_t *moved, DreamSocketError *error) {
    int64_t got = recv(socket, (char *)bytes, (socket_size)count, 0);
    if (got >= 0) {
        *moved = got;
        return DREAM_IO_DONE;
    }
    return io_failed(socket_errno(), "cannot read socket", error);
}

int dream_socket_send(dream_socket socket, const uint8_t *bytes, int32_t count, int64_t *moved,
                      DreamSocketError *error) {
    int64_t sent = send(socket, (const char *)bytes, (socket_size)count, SEND_FLAGS);
    if (sent >= 0) {
        *moved = sent;
        return DREAM_IO_DONE;
    }
    return io_failed(socket_errno(), "cannot write socket", error);
}

int dream_socket_recvfrom(dream_socket socket, uint8_t *bytes, int32_t count, int64_t *moved,
                          DreamSocketAddress *peer, DreamSocketError *error) {
    peer->length = sizeof(peer->storage);
    int64_t got = recvfrom(socket, (char *)bytes, (socket_size)count, 0, (struct sockaddr *)&peer->storage,
                           &peer->length);
    if (got >= 0) {
        *moved = got;
        return DREAM_IO_DONE;
    }
    return io_failed(socket_errno(), "cannot receive datagram", error);
}

int dream_socket_sendto(dream_socket socket, const uint8_t *bytes, int32_t count, const DreamSocketAddress *target,
                        int64_t *moved, DreamSocketError *error) {
    int64_t sent = sendto(socket, (const char *)bytes, (socket_size)count, SEND_FLAGS,
                          (const struct sockaddr *)&target->storage, target->length);
    if (sent >= 0) {
        *moved = sent;
        return DREAM_IO_DONE;
    }
    return io_failed(socket_errno(), "cannot send datagram", error);
}

int dream_socket_shutdown_write(dream_socket socket, DreamSocketError *error) {
#ifdef _WIN32
    int how = SD_SEND;
#else
    int how = SHUT_WR;
#endif
    if (shutdown(socket, how) == 0) { return 0; }
    fail_system(error, socket_errno(), "cannot shut down socket");
    return -1;
}

int dream_socket_set_option(dream_socket socket, int option, int value, DreamSocketError *error) {
    int result = -1;
    if (option == OPTION_NODELAY) {
        result = setsockopt(socket, IPPROTO_TCP, TCP_NODELAY, (const char *)&value, sizeof(value));
    } else if (option == OPTION_BROADCAST) {
#ifdef SO_BROADCAST
        result = setsockopt(socket, SOL_SOCKET, SO_BROADCAST, (const char *)&value, sizeof(value));
#endif
    } else if (option == OPTION_TTL) {
#ifdef IPV6_UNICAST_HOPS
        if (dream_socket_family(socket) == AF_INET6) {
            result = setsockopt(socket, IPPROTO_IPV6, IPV6_UNICAST_HOPS, (const char *)&value, sizeof(value));
        } else
#endif
        {
            result = setsockopt(socket, IPPROTO_IP, IP_TTL, (const char *)&value, sizeof(value));
        }
    } else {
        dream_socket_fail(error, "EINVAL", "unsupported socket option");
        return -1;
    }
    if (result == 0) { return 0; }
    fail_system(error, socket_errno(), "cannot set socket option");
    return -1;
}

int dream_socket_family(dream_socket socket) {
    struct sockaddr_storage address;
    socklen_t length = sizeof(address);
    memset(&address, 0, sizeof(address));
    if (getsockname(socket, (struct sockaddr *)&address, &length) != 0) { return AF_UNSPEC; }
    return address.ss_family;
}

int dream_socket_address_text(const DreamSocketAddress *address, char *host, size_t length) {
    host[0] = 0;
    if (address->storage.ss_family == AF_INET) {
        const struct sockaddr_in *value = (const struct sockaddr_in *)&address->storage;
        inet_ntop(AF_INET, (void *)&value->sin_addr, host, (socklen_t)length);
        return ntohs(value->sin_port);
    }
    if (address->storage.ss_family == AF_INET6) {
        const struct sockaddr_in6 *value = (const struct sockaddr_in6 *)&address->storage;
        inet_ntop(AF_INET6, (void *)&value->sin6_addr, host, (socklen_t)length);
        return ntohs(value->sin6_port);
    }
    return -1;
}

int dream_socket_endpoint(dream_socket socket, int peer, char *host, size_t length) {
    DreamSocketAddress address;
    memset(&address, 0, sizeof(address));
    address.length = sizeof(address.storage);
    int result = peer ? getpeername(socket, (struct sockaddr *)&address.storage, &address.length)
                      : getsockname(socket, (struct sockaddr *)&address.storage, &address.length);
    if (result != 0) {
        host[0] = 0;
        return -1;
    }
    return dream_socket_address_text(&address, host, length);
}

int dream_socket_wait(dream_socket socket, int write, DreamSocketWake *wake, int timeout_ms) {
    dream_pollfd fds[2];
    memset(fds, 0, sizeof(fds));
    fds[0].fd = socket;
    fds[0].events = write ? POLLOUT : POLLIN;
    int count = 1;
    if (wake) {
#ifdef _WIN32
        fds[1].fd = wake->socket;
#else
        fds[1].fd = wake->fds[0];
#endif
        fds[1].events = POLLIN;
        count = 2;
    }
    for (;;) {
        int ready = dream_poll(fds, count, timeout_ms < 0 ? -1 : timeout_ms);
        if (ready < 0 && SOCKET_INTERRUPTED(socket_errno())) { continue; }
        if (ready < 0) { return DREAM_WAIT_FAILED; }
        if (ready == 0) { return DREAM_WAIT_TIMEOUT; }
        if (count == 2 && fds[1].revents) { return DREAM_WAIT_WOKEN; }
        return DREAM_WAIT_READY;
    }
}

#ifdef _WIN32
int dream_socket_wake_open(DreamSocketWake *wake) {
    wake->socket = WSASocketW(AF_INET, SOCK_DGRAM, IPPROTO_UDP, NULL, 0, WSA_FLAG_NO_HANDLE_INHERIT);
    if (wake->socket == INVALID_SOCKET) { return -1; }
    struct sockaddr_in loopback;
    memset(&loopback, 0, sizeof(loopback));
    loopback.sin_family = AF_INET;
    loopback.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(wake->socket, (struct sockaddr *)&loopback, sizeof(loopback)) != 0 || set_nonblocking(wake->socket) != 0) {
        closesocket(wake->socket);
        wake->socket = INVALID_SOCKET;
        return -1;
    }
    return 0;
}

void dream_socket_wake_signal(DreamSocketWake *wake) {
    struct sockaddr_in self;
    int length = sizeof(self);
    if (getsockname(wake->socket, (struct sockaddr *)&self, &length) == 0) {
        char byte = 1;
        sendto(wake->socket, &byte, 1, 0, (struct sockaddr *)&self, length);
    }
}

void dream_socket_wake_close(DreamSocketWake *wake) {
    if (wake->socket != INVALID_SOCKET) { closesocket(wake->socket); }
    wake->socket = INVALID_SOCKET;
}
#elif !defined(__wasi__)
int dream_socket_wake_open(DreamSocketWake *wake) {
#if defined(__linux__)
    return pipe2(wake->fds, O_CLOEXEC | O_NONBLOCK);
#else
    if (pipe(wake->fds) != 0) { return -1; }
    for (int i = 0; i < 2; ++i) {
        fcntl(wake->fds[i], F_SETFD, FD_CLOEXEC);
        fcntl(wake->fds[i], F_SETFL, fcntl(wake->fds[i], F_GETFL, 0) | O_NONBLOCK);
    }
    return 0;
#endif
}

void dream_socket_wake_signal(DreamSocketWake *wake) {
    char byte = 1;
    ssize_t written = write(wake->fds[1], &byte, 1);
    (void)written;
}

void dream_socket_wake_close(DreamSocketWake *wake) {
    for (int i = 0; i < 2; ++i) {
        if (wake->fds[i] >= 0) { close(wake->fds[i]); }
        wake->fds[i] = -1;
    }
}
#endif
