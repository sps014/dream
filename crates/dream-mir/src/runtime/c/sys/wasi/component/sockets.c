#include "network.h"
#include "socket_lookup.h"
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <errno.h>
#include <poll.h>
#include <fcntl.h>

static struct { int live, fd, kind; } sockets[COMPONENT_LIMIT];
typedef struct {
    int live, child, connecting;
    int64_t deadline;
    dream_ptr result, waiter, bytes;
    yyjson_doc *doc;
    SocketLookup *lookup;
    struct sockaddr_storage address;
    socklen_t address_length;
} SocketOperation;

static SocketOperation operations[COMPONENT_LIMIT];
static int next_socket = 1, next_operation = 1, waiting_count;
int64_t timeNowNanos(void);

static int endpoint_text(const struct sockaddr_storage *address, char *host, size_t length) {
    if (address->ss_family == AF_INET) {
        const struct sockaddr_in *value = (const struct sockaddr_in *)address;
        inet_ntop(AF_INET, &value->sin_addr, host, (socklen_t)length);
        return ntohs(value->sin_port);
    }
    if (address->ss_family == AF_INET6) {
        const struct sockaddr_in6 *value = (const struct sockaddr_in6 *)address;
        inet_ntop(AF_INET6, &value->sin6_addr, host, (socklen_t)length);
        return ntohs(value->sin6_port);
    }
    return 0;
}

static int fd_of(int id) { return id > 0 && id < COMPONENT_LIMIT && sockets[id].live ? sockets[id].fd : -1; }
void netSocketClose(int32_t id) {
    int fd = fd_of(id);
    if (fd >= 0) { sockets[id].live = 0; close(fd); }
}

static dream_ptr metadata(int id) {
    struct sockaddr_storage local = {0}, peer = {0};
    socklen_t local_size = sizeof(local), peer_size = sizeof(peer);
    char local_host[NI_MAXHOST] = "", peer_host[NI_MAXHOST] = "";
    int local_port = 0, peer_port = 0;
    int fd = fd_of(id);
    if (!getsockname(fd, (struct sockaddr *)&local, &local_size)) {
        local_port = endpoint_text(&local, local_host, sizeof(local_host));
    }
    if (!getpeername(fd, (struct sockaddr *)&peer, &peer_size)) {
        peer_port = endpoint_text(&peer, peer_host, sizeof(peer_host));
    }
    yyjson_mut_doc *doc = yyjson_mut_doc_new(NULL);
    yyjson_mut_val *root = yyjson_mut_obj(doc);
    yyjson_mut_doc_set_root(doc, root);
    yyjson_mut_obj_add_int(doc, root, "id", id);
    yyjson_mut_obj_add_str(doc, root, "local_host", local_host);
    yyjson_mut_obj_add_int(doc, root, "local_port", local_port);
    yyjson_mut_obj_add_str(doc, root, "peer_host", peer_host);
    yyjson_mut_obj_add_int(doc, root, "peer_port", peer_port);
    dream_ptr result = component_json(doc, 1); yyjson_mut_doc_free(doc); return result;
}

static int register_socket(int fd, int kind) {
    if (fd < 0 || next_socket == COMPONENT_LIMIT) { if (fd >= 0) { close(fd); } return 0; }
    int id = next_socket++;
    sockets[id].live = 1; sockets[id].fd = fd; sockets[id].kind = kind; return id;
}

static void clear_pending(SocketOperation *op) {
    if (op->connecting >= 0) { close(op->connecting); op->connecting = -1; }
    component_socket_lookup_drop(op->lookup);
    op->lookup = NULL;
    yyjson_doc_free(op->doc);
    op->doc = NULL;
    dream_release(op->bytes);
    op->bytes = 0;
}

static dream_ptr open_socket(SocketOperation *op, yyjson_val *request, const char *action) {
    if (op->connecting >= 0) {
        struct pollfd wait = {op->connecting, POLLOUT, 0};
        int ready = poll(&wait, 1, 0);
        if (!ready) { return 0; }
        int failure = 0;
        socklen_t size = sizeof(failure);
        if (ready > 0 && !getsockopt(op->connecting, SOL_SOCKET, SO_ERROR, &failure, &size)
            && !failure) {
            op->child = register_socket(op->connecting, 0);
            op->connecting = -1;
            return op->child ? metadata(op->child) : component_error("ELIMIT", "socket capacity exhausted");
        }
        close(op->connecting);
        op->connecting = -1;
    }
    int datagram = !strcmp(action, "udp_bind"), connecting = !strcmp(action, "connect");
    if (!op->lookup) {
        const char *host = yyjson_get_str(yyjson_obj_get(request, "host"));
        int64_t port = yyjson_get_sint(yyjson_obj_get(request, "port"));
        if (!host || port < 0 || port > 65535) { return component_error("EINVAL", "invalid endpoint"); }
        op->lookup = component_socket_lookup_start(host, (uint16_t)port);
        if (!op->lookup) { return component_error("EIO", "WASI name lookup failed"); }
    }
    for (;;) {
        int state = component_socket_lookup_next(op->lookup, &op->address, &op->address_length);
        if (state == LOOKUP_PENDING) { return 0; }
        if (state == LOOKUP_FAILED) { return component_error("EIO", "WASI name lookup failed"); }
        if (state == LOOKUP_END) { return component_error("EIO", "WASI socket open failed"); }
        int fd = socket(op->address.ss_family, datagram ? SOCK_DGRAM : SOCK_STREAM, 0);
        if (fd < 0) { continue; }
        if (fcntl(fd, F_SETFL, O_NONBLOCK) < 0) { close(fd); continue; }
        const struct sockaddr *address = (const struct sockaddr *)&op->address;
        int result = connecting ? connect(fd, address, op->address_length)
            : bind(fd, address, op->address_length);
        if (result < 0 && connecting && (errno == EINPROGRESS || errno == EWOULDBLOCK)) {
            op->connecting = fd;
            return 0;
        }
        if (!result && (connecting || datagram || !listen(fd, 128))) {
            op->child = register_socket(fd, datagram ? 2 : connecting ? 0 : 1);
            return op->child ? metadata(op->child) : component_error("ELIMIT", "socket capacity exhausted");
        }
        close(fd);
    }
}

static int would_block(void) { return errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR; }

static dream_ptr operate(SocketOperation *op) {
    yyjson_val *request = yyjson_doc_get_root(op->doc);
    const char *action = yyjson_get_str(yyjson_obj_get(request, "action"));
    if (!action) { return component_error("EINVAL", "missing socket action"); }
    if (!strcmp(action, "connect") || !strcmp(action, "listen") || !strcmp(action, "udp_bind")) {
        return open_socket(op, request, action);
    }
    int id = (int)yyjson_get_sint(yyjson_obj_get(request, "id"));
    int fd = fd_of(id);
    if (fd < 0) { return component_error("ECLOSED", "socket is closed"); }
    dream_ptr bytes = op->bytes;
    if (!strcmp(action, "accept")) {
        int accepted = accept(fd, NULL, NULL);
        if (accepted < 0) { return would_block() ? 0 : component_error("EIO", "socket accept failed"); }
        if (fcntl(accepted, F_SETFL, O_NONBLOCK) < 0) {
            close(accepted);
            return component_error("EIO", "socket accept configuration failed");
        }
        op->child = register_socket(accepted, 0);
        return op->child ? metadata(op->child) : component_error("ELIMIT", "socket capacity exhausted");
    }
    if (!strcmp(action, "read")) {
        int64_t count = yyjson_get_sint(yyjson_obj_get(request, "count"));
        if (count < 0) { return component_error("EINVAL", "invalid read size"); }
        /* Larger reads return short so a huge request does not preallocate its buffer. */
        if (count > 1024 * 1024) { count = 1024 * 1024; }
        uint8_t *buffer = malloc((size_t)count + 1);
        if (!buffer) { return component_error("ELIMIT", "socket allocation failed"); }
        ssize_t n = count ? recv(fd, buffer + 1, (size_t)count, 0) : 0;
        buffer[0] = 1;
        dream_ptr result = n < 0 ? (would_block() ? 0 : component_error("EIO", "socket read failed"))
            : component_bytes(buffer, (size_t)n + 1);
        free(buffer); return result;
    }
    if (!strcmp(action, "write")) {
        size_t count = bytes ? (size_t)dream_i32(bytes)[0] : 0;
        ssize_t written = count ? send(fd, dream_array_bytes(bytes), count, 0) : 0;
        if (written < 0) { return would_block() ? 0 : component_error("EIO", "socket write failed"); }
        char result[32]; result[0] = 1;
        int n = snprintf(result + 1, sizeof(result) - 1, "%lld", (long long)written);
        return component_bytes(result, (size_t)n + 1);
    }
    if (!strcmp(action, "udp_send")) {
        const char *host = yyjson_get_str(yyjson_obj_get(request, "host"));
        int64_t port = yyjson_get_sint(yyjson_obj_get(request, "port"));
        size_t count = bytes ? (size_t)dream_i32(bytes)[0] : 0;
        if (!host || port < 0 || port > 65535 || count > 65507) { return component_error("EINVAL", "invalid datagram"); }
        if (!op->lookup) {
            op->lookup = component_socket_lookup_start(host, (uint16_t)port);
            if (!op->lookup) { return component_error("EIO", "WASI name lookup failed"); }
        }
        ssize_t count_sent;
        for (;;) {
            if (!op->address_length) {
                int state = component_socket_lookup_next(op->lookup, &op->address, &op->address_length);
                if (state == LOOKUP_PENDING) { return 0; }
                if (state == LOOKUP_FAILED) { return component_error("EIO", "WASI name lookup failed"); }
                if (state == LOOKUP_END) { return component_error("EIO", "WASI datagram send failed"); }
            }
            count_sent = sendto(fd, bytes ? dream_array_bytes(bytes) : NULL, count, 0,
                (const struct sockaddr *)&op->address, op->address_length);
            if (count_sent >= 0) { break; }
            if (would_block()) { return 0; }
            op->address_length = 0;
        }
        char result[32]; result[0] = 1;
        int n = snprintf(result + 1, sizeof(result) - 1, "%lld", (long long)count_sent);
        return component_bytes(result, (size_t)n + 1);
    }
    if (!strcmp(action, "udp_receive")) {
        int64_t count = yyjson_get_sint(yyjson_obj_get(request, "count"));
        if (count < 0 || count > 65535) { return component_error("EINVAL", "invalid datagram read size"); }
        uint8_t *buffer = malloc(65536);
        if (!buffer) { return component_error("ELIMIT", "datagram allocation failed"); }
        struct sockaddr_storage peer = {0}; socklen_t size = sizeof(peer);
        ssize_t received = recvfrom(fd, buffer, 65536, 0, (struct sockaddr *)&peer, &size);
        if (received < 0) {
            int pending = would_block();
            free(buffer);
            return pending ? 0 : component_error("EIO", "datagram receive failed");
        }
        size_t length = (size_t)(received < count ? received : count);
        char *hex = malloc(length * 2 + 1);
        if (!hex) { free(buffer); return component_error("ELIMIT", "datagram allocation failed"); }
        static const char digits[] = "0123456789abcdef";
        for (size_t i = 0; i < length; i++) { hex[i * 2] = digits[buffer[i] >> 4]; hex[i * 2 + 1] = digits[buffer[i] & 15]; }
        hex[length * 2] = 0; free(buffer);
        char host[NI_MAXHOST] = "";
        int port = endpoint_text(&peer, host, sizeof(host));
        yyjson_mut_doc *doc = yyjson_mut_doc_new(NULL); yyjson_mut_val *root = yyjson_mut_obj(doc);
        yyjson_mut_doc_set_root(doc, root);
        yyjson_mut_obj_add_str(doc, root, "host", host); yyjson_mut_obj_add_int(doc, root, "port", port);
        yyjson_mut_obj_add_str(doc, root, "bytes", hex); yyjson_mut_obj_add_bool(doc, root, "truncated", received > count);
        dream_ptr result = component_json(doc, 1); yyjson_mut_doc_free(doc); free(hex); return result;
    }
    return component_error("ENOTSUP", "socket operation is unavailable");
}

int32_t netSocketStart(dream_ptr request, dream_ptr bytes) {
    if (next_operation == COMPONENT_LIMIT) { return 0; }
    int id = next_operation++;
    SocketOperation *op = &operations[id];
    op->live = 1;
    op->connecting = -1;
    char *text = dream_str_utf8(request);
    op->doc = text ? yyjson_read(text, strlen(text), 0) : NULL;
    free(text);
    if (!op->doc) { op->result = component_error("EINVAL", "invalid socket request"); return id; }
    int64_t timeout = yyjson_get_sint(yyjson_obj_get(yyjson_doc_get_root(op->doc), "timeout_ms"));
    if (timeout > 0) { op->deadline = timeNowNanos() + timeout * 1000000; }
    op->bytes = component_copy_bytes(bytes);
    return id;
}

static void wake_waiter(SocketOperation *op) {
    dream_ptr future = op->waiter;
    if (future) {
        op->waiter = 0;
        waiting_count--;
        dream_async_complete(future, 0);
        dream_release(future);
    }
}

static int poll_sockets(void) {
    if (!waiting_count) { return component_socket_lookup_cleanup(); }
    for (int i = 1; i < next_operation; i++) {
        SocketOperation *op = &operations[i];
        if (!op->waiter) { continue; }
        if (dream_async_host_status(op->waiter)) {
            op->result = component_error("ECANCELLED", "socket operation cancelled");
        } else if (op->deadline && timeNowNanos() >= op->deadline) {
            op->result = component_error("ETIMEOUT", "socket operation timed out");
        } else {
            op->result = operate(op);
        }
        if (op->result) {
            clear_pending(op);
            wake_waiter(op);
        }
    }
    int cleanup_pending = component_socket_lookup_cleanup();
    return waiting_count || cleanup_pending;
}

void netSocketWait(dream_ptr future, int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live
        || !operations[id].doc || operations[id].waiter) {
        dream_async_complete(future, 0);
        return;
    }
    SocketOperation *op = &operations[id];
    dream_retain(future);
    op->waiter = future;
    if (waiting_count++ == 0) { dream_component_register_sockets(poll_sockets); }
}

dream_ptr netSocketResult(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return component_error("ECLOSED", "socket operation is closed"); }
    dream_retain(operations[id].result); return operations[id].result;
}
void netSocketClaim(int32_t id) { if (id > 0 && id < COMPONENT_LIMIT) { operations[id].child = 0; } }
void netSocketRelease(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return; }
    SocketOperation *op = &operations[id];
    op->live = 0;
    clear_pending(op);
    wake_waiter(op);
    dream_release(op->result);
    if (op->child) { netSocketClose(op->child); }
}
void netSocketCancel(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live || !operations[id].doc) { return; }
    SocketOperation *op = &operations[id];
    clear_pending(op);
    op->result = component_error("ECANCELLED", "socket operation cancelled");
    wake_waiter(op);
}
dream_ptr netSocketControl(int32_t id, dream_ptr request) {
    char *text = dream_str_utf8(request);
    yyjson_doc *doc = text ? yyjson_read(text, strlen(text), 0) : NULL;
    yyjson_val *root = doc ? yyjson_doc_get_root(doc) : NULL;
    const char *action = yyjson_get_str(yyjson_obj_get(root, "action"));
    int fd = fd_of(id), result = -1;
    if (fd >= 0 && action) {
        int enabled = yyjson_get_bool(yyjson_obj_get(root, "value"));
        if (!strcmp(action, "shutdown_write")) { result = shutdown(fd, SHUT_WR); }
        else if (!strcmp(action, "nodelay")) { result = setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &enabled, sizeof(enabled)); }
#ifdef SO_BROADCAST
        else if (!strcmp(action, "broadcast")) { result = setsockopt(fd, SOL_SOCKET, SO_BROADCAST, &enabled, sizeof(enabled)); }
#endif
        else if (!strcmp(action, "ttl")) {
            int ttl = (int)yyjson_get_sint(yyjson_obj_get(root, "value"));
            if (ttl > 0 && ttl <= 255) { result = setsockopt(fd, IPPROTO_IP, IP_TTL, &ttl, sizeof(ttl)); }
        }
    }
    yyjson_doc_free(doc); free(text);
    return dream_str_from_utf8(result ? "{\"code\":\"ENOTSUP\",\"message\":\"socket control unavailable\"}" : "{}");
}
