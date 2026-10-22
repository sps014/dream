#include "network.h"
#include <sys/socket.h>
#include <netdb.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <errno.h>
#include <poll.h>

static struct { int live, fd, kind; } sockets[COMPONENT_LIMIT];
static struct { int live, child; dream_ptr result; } operations[COMPONENT_LIMIT];
static int next_socket = 1, next_operation = 1;

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

static dream_ptr operate(yyjson_val *request, dream_ptr bytes, int *child) {
    const char *action = yyjson_get_str(yyjson_obj_get(request, "action"));
    if (!action) { return component_error("EINVAL", "missing socket action"); }
    int id = (int)yyjson_get_sint(yyjson_obj_get(request, "id"));
    int fd = fd_of(id);
    int timeout = (int)yyjson_get_sint(yyjson_obj_get(request, "timeout_ms"));
    if (!strcmp(action, "connect") || !strcmp(action, "listen") || !strcmp(action, "udp_bind")) {
        const char *host = yyjson_get_str(yyjson_obj_get(request, "host"));
        int port = (int)yyjson_get_sint(yyjson_obj_get(request, "port"));
        if (!host || port < 0 || port > 65535) { return component_error("EINVAL", "invalid endpoint"); }
        char service[8]; snprintf(service, sizeof(service), "%d", port);
        int datagram = !strcmp(action, "udp_bind"), connecting = !strcmp(action, "connect");
        struct addrinfo hints = {0}, *addresses = NULL;
        hints.ai_socktype = datagram ? SOCK_DGRAM : SOCK_STREAM;
        hints.ai_family = AF_UNSPEC;
        if (getaddrinfo(host, service, &hints, &addresses)) { return component_error("EIO", "WASI name lookup failed"); }
        int opened = -1;
        for (struct addrinfo *address = addresses; address; address = address->ai_next) {
            opened = socket(address->ai_family, address->ai_socktype, address->ai_protocol);
            if (opened < 0) { continue; }
            int ok = connecting ? connect(opened, address->ai_addr, address->ai_addrlen)
                : bind(opened, address->ai_addr, address->ai_addrlen);
            if (!ok && !connecting && !datagram) { ok = listen(opened, 128); }
            if (!ok) { break; }
            close(opened); opened = -1;
        }
        freeaddrinfo(addresses);
        if (opened < 0) { return component_error("EIO", "WASI socket open failed"); }
        *child = register_socket(opened, datagram ? 2 : connecting ? 0 : 1);
        return *child ? metadata(*child) : component_error("ELIMIT", "socket capacity exhausted");
    }
    if (fd < 0) { return component_error("ECLOSED", "socket is closed"); }
    if (timeout > 0) {
        struct pollfd wait = {fd, !strcmp(action, "write") || !strcmp(action, "udp_send") ? POLLOUT : POLLIN, 0};
        int ready = poll(&wait, 1, timeout);
        if (ready == 0) { return component_error("ETIMEOUT", "socket operation timed out"); }
        if (ready < 0) { return component_error("EIO", "socket poll failed"); }
    }
    if (!strcmp(action, "accept")) {
        int accepted = accept(fd, NULL, NULL);
        if (accepted < 0) { return component_error("EIO", "socket accept failed"); }
        *child = register_socket(accepted, 0);
        return *child ? metadata(*child) : component_error("ELIMIT", "socket capacity exhausted");
    }
    if (!strcmp(action, "read")) {
        int64_t count = yyjson_get_sint(yyjson_obj_get(request, "count"));
        if (count < 0 || count > 16 * 1024 * 1024) { return component_error("EINVAL", "invalid read size"); }
        uint8_t *buffer = malloc((size_t)count + 1);
        if (!buffer) { return component_error("ELIMIT", "socket allocation failed"); }
        ssize_t n = count ? recv(fd, buffer + 1, (size_t)count, 0) : 0;
        buffer[0] = 1;
        dream_ptr result = n < 0 ? component_error("EIO", "socket read failed") : component_bytes(buffer, (size_t)n + 1);
        free(buffer); return result;
    }
    if (!strcmp(action, "write")) {
        size_t count = bytes ? (size_t)dream_i32(bytes)[0] : 0;
        if (count > 16 * 1024 * 1024) { return component_error("EINVAL", "invalid write size"); }
        ssize_t written = count ? send(fd, dream_array_bytes(bytes), count, 0) : 0;
        if (written < 0) { return component_error("EIO", "socket write failed"); }
        char result[32]; result[0] = 1;
        int n = snprintf(result + 1, sizeof(result) - 1, "%lld", (long long)written);
        return component_bytes(result, (size_t)n + 1);
    }
    if (!strcmp(action, "udp_send")) {
        const char *host = yyjson_get_str(yyjson_obj_get(request, "host"));
        int port = (int)yyjson_get_sint(yyjson_obj_get(request, "port"));
        size_t count = bytes ? (size_t)dream_i32(bytes)[0] : 0;
        if (!host || port < 0 || port > 65535 || count > 65507) { return component_error("EINVAL", "invalid datagram"); }
        char service[8]; snprintf(service, sizeof(service), "%d", port);
        struct addrinfo hints = {0}, *addresses = NULL;
        hints.ai_socktype = SOCK_DGRAM; hints.ai_family = AF_UNSPEC;
        if (getaddrinfo(host, service, &hints, &addresses)) { return component_error("EIO", "WASI name lookup failed"); }
        ssize_t count_sent = -1;
        for (struct addrinfo *address = addresses; address; address = address->ai_next) {
            count_sent = sendto(fd, bytes ? dream_array_bytes(bytes) : NULL, count, 0, address->ai_addr, address->ai_addrlen);
            if (count_sent >= 0) { break; }
        }
        freeaddrinfo(addresses);
        if (count_sent < 0) { return component_error("EIO", strerror(errno)); }
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
        if (received < 0) { free(buffer); return component_error("EIO", "datagram receive failed"); }
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
    operations[id].live = 1;
    char *text = dream_str_utf8(request);
    yyjson_doc *doc = text ? yyjson_read(text, strlen(text), 0) : NULL;
    operations[id].result = doc ? operate(yyjson_doc_get_root(doc), bytes, &operations[id].child)
        : component_error("EINVAL", "invalid socket request");
    yyjson_doc_free(doc); free(text); return id;
}
void netSocketWait(dream_ptr future, int32_t id) { (void)id; dream_async_complete(future, 0); }
dream_ptr netSocketResult(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return component_error("ECLOSED", "socket operation is closed"); }
    dream_retain(operations[id].result); return operations[id].result;
}
void netSocketClaim(int32_t id) { if (id > 0 && id < COMPONENT_LIMIT) { operations[id].child = 0; } }
void netSocketRelease(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return; }
    operations[id].live = 0; dream_release(operations[id].result);
    if (operations[id].child) { netSocketClose(operations[id].child); }
}
void netSocketCancel(int32_t id) { netSocketRelease(id); }
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
