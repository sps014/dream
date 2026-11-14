#ifndef DREAM_SOCKET_H
#define DREAM_SOCKET_H

/* Operating-system sockets for system.net: BSD sockets on POSIX and WASI, Winsock on Windows.
 * Every socket is nonblocking and not inherited by child processes; callers wait for readiness
 * with dream_socket_wait. */

#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0600
#endif
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <winsock2.h>
#include <ws2tcpip.h>
typedef SOCKET dream_socket;
#define DREAM_SOCKET_NONE INVALID_SOCKET
#else
#include <netdb.h>
#include <sys/socket.h>
typedef int dream_socket;
#define DREAM_SOCKET_NONE (-1)
#endif

enum { DREAM_IO_DONE, DREAM_IO_BLOCKED, DREAM_IO_FAILED };
enum { DREAM_WAIT_READY, DREAM_WAIT_WOKEN, DREAM_WAIT_TIMEOUT, DREAM_WAIT_FAILED };

typedef struct {
    const char *code;
    char message[160];
} DreamSocketError;

/* Interrupts a blocked dream_socket_wait from another thread: a pipe on POSIX and a loopback
 * datagram socket on Windows, where WSAPoll accepts only sockets. */
typedef struct {
#ifdef _WIN32
    SOCKET socket;
#else
    int fds[2];
#endif
} DreamSocketWake;

typedef struct {
    struct sockaddr_storage storage;
    socklen_t length;
} DreamSocketAddress;

int dream_socket_startup(DreamSocketError *error);
void dream_socket_close(dream_socket socket);
void dream_socket_fail(DreamSocketError *error, const char *code, const char *message);

/* Resolves `host` for a stream or datagram socket; `family` restricts results unless it is
 * AF_UNSPEC. With `numeric`, returns 1 instead of blocking when the host needs a name lookup. */
int dream_socket_resolve(const char *host, int port, int datagram, int numeric, int family,
                         struct addrinfo **addresses, DreamSocketError *error);

dream_socket dream_socket_open(const struct addrinfo *address, DreamSocketError *error);
int dream_socket_connect_start(dream_socket socket, const struct addrinfo *address, DreamSocketError *error);
int dream_socket_connect_finish(dream_socket socket, DreamSocketError *error);
int dream_socket_listen(dream_socket socket, const struct addrinfo *address, int stream, DreamSocketError *error);

int dream_socket_accept(dream_socket socket, dream_socket *accepted, DreamSocketError *error);
int dream_socket_recv(dream_socket socket, uint8_t *bytes, int32_t count, int64_t *moved, DreamSocketError *error);
int dream_socket_send(dream_socket socket, const uint8_t *bytes, int32_t count, int64_t *moved, DreamSocketError *error);
int dream_socket_recvfrom(dream_socket socket, uint8_t *bytes, int32_t count, int64_t *moved,
                          DreamSocketAddress *peer, DreamSocketError *error);
int dream_socket_sendto(dream_socket socket, const uint8_t *bytes, int32_t count, const DreamSocketAddress *target,
                        int64_t *moved, DreamSocketError *error);

int dream_socket_shutdown_write(dream_socket socket, DreamSocketError *error);
int dream_socket_set_option(dream_socket socket, int option, int value, DreamSocketError *error);
int dream_socket_family(dream_socket socket);

/* Formats the numeric address and returns the port, or returns -1 when the endpoint is unknown. */
int dream_socket_endpoint(dream_socket socket, int peer, char *host, size_t length);
int dream_socket_address_text(const DreamSocketAddress *address, char *host, size_t length);

/* Waits until `socket` can read (or write), `wake` fires, or `timeout_ms` (negative: forever)
 * passes. `wake` may be NULL. */
int dream_socket_wait(dream_socket socket, int write, DreamSocketWake *wake, int timeout_ms);
int dream_socket_wake_open(DreamSocketWake *wake);
void dream_socket_wake_signal(DreamSocketWake *wake);
void dream_socket_wake_close(DreamSocketWake *wake);

#endif
