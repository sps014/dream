#include "socket_lookup.h"
#include <arpa/inet.h>
#include <stdlib.h>
#include <string.h>

#ifdef __wasip3__
#include "p3/dream_http_p3.h"
typedef wasi_sockets_ip_name_lookup_ip_address_t IpAddress;
#define IPV4 WASI_SOCKETS_TYPES_IP_ADDRESS_IPV4
#else
#include <wasi/wasip2.h>
typedef ip_name_lookup_ip_address_t IpAddress;
#define IPV4 NETWORK_IP_ADDRESS_IPV4
#endif

struct SocketLookup {
    struct sockaddr_storage numeric;
    socklen_t numeric_size;
    uint16_t port;
#ifdef __wasip3__
    SocketLookup *next;
    char *host;
    dream_http_p3_subtask_status_t status;
    dream_http_p3_waitable_set_t set;
    wasi_sockets_ip_name_lookup_result_list_ip_address_error_code_t result;
    size_t index;
#else
    instance_network_own_network_t network;
    ip_name_lookup_own_resolve_address_stream_t stream;
    ip_name_lookup_own_pollable_t ready;
#endif
};

SocketLookup *component_socket_lookup_start(const char *host, uint16_t port) {
    struct sockaddr_in ipv4 = {.sin_family = AF_INET, .sin_port = htons(port)};
    struct sockaddr_in6 ipv6 = {.sin6_family = AF_INET6, .sin6_port = htons(port)};
    SocketLookup *lookup = calloc(1, sizeof(*lookup));
    if (!lookup) { return NULL; }
    lookup->port = port;
    if (inet_pton(AF_INET, host, &ipv4.sin_addr) == 1) {
        memcpy(&lookup->numeric, &ipv4, sizeof(ipv4));
        lookup->numeric_size = sizeof(ipv4);
    } else if (inet_pton(AF_INET6, host, &ipv6.sin6_addr) == 1) {
        memcpy(&lookup->numeric, &ipv6, sizeof(ipv6));
        lookup->numeric_size = sizeof(ipv6);
    } else {
#ifdef __wasip3__
        lookup->host = strdup(host);
        if (!lookup->host) { free(lookup); return NULL; }
        dream_http_p3_string_t name = {(uint8_t *)lookup->host, strlen(host)};
        lookup->status = wasi_sockets_ip_name_lookup_resolve_addresses(name, &lookup->result);
#else
        wasip2_string_t name = {(uint8_t *)host, strlen(host)};
        lookup->network = instance_network_instance_network();
        ip_name_lookup_error_code_t error;
        if (!ip_name_lookup_resolve_addresses(network_borrow_network(lookup->network), &name,
            &lookup->stream, &error)) {
            network_network_drop_own(lookup->network);
            free(lookup);
            return NULL;
        }
        lookup->ready = ip_name_lookup_method_resolve_address_stream_subscribe(
            ip_name_lookup_borrow_resolve_address_stream(lookup->stream));
#endif
    }
    return lookup;
}

#ifdef __wasip3__
static SocketLookup *cancelled;

static dream_http_p3_subtask_state_t poll_lookup(SocketLookup *lookup) {
    dream_http_p3_subtask_state_t state = DREAM_HTTP_P3_SUBTASK_STATE(lookup->status);
    if (state == DREAM_HTTP_P3_SUBTASK_RETURNED || state == DREAM_HTTP_P3_SUBTASK_RETURNED_CANCELLED) {
        return state;
    }
    dream_http_p3_subtask_t task = DREAM_HTTP_P3_SUBTASK_HANDLE(lookup->status);
    if (!lookup->set) {
        lookup->set = dream_http_p3_waitable_set_new();
        dream_http_p3_waitable_join(task, lookup->set);
    }
    dream_http_p3_event_t event;
    dream_http_p3_waitable_set_poll(lookup->set, &event);
    if (event.event == DREAM_HTTP_P3_EVENT_SUBTASK) {
        state = event.code;
        lookup->status = (task << 4) | state;
    }
    return state;
}

static void free_lookup(SocketLookup *lookup) {
    dream_http_p3_subtask_t task = DREAM_HTTP_P3_SUBTASK_HANDLE(lookup->status);
    if (lookup->set) {
        dream_http_p3_waitable_join(task, 0);
        dream_http_p3_waitable_set_drop(lookup->set);
    }
    if (task) { dream_http_p3_subtask_drop(task); }
    if (!lookup->numeric.ss_family && DREAM_HTTP_P3_SUBTASK_STATE(lookup->status) == DREAM_HTTP_P3_SUBTASK_RETURNED) {
        wasi_sockets_ip_name_lookup_result_list_ip_address_error_code_free(&lookup->result);
    }
    free(lookup->host);
    free(lookup);
}

void component_socket_lookup_drop(SocketLookup *lookup) {
    if (!lookup) { return; }
    dream_http_p3_subtask_t task = DREAM_HTTP_P3_SUBTASK_HANDLE(lookup->status);
    dream_http_p3_subtask_state_t state = DREAM_HTTP_P3_SUBTASK_STATE(lookup->status);
    if (task && (state == DREAM_HTTP_P3_SUBTASK_STARTING || state == DREAM_HTTP_P3_SUBTASK_STARTED)) {
        state = DREAM_HTTP_P3_SUBTASK_STATE(dream_http_p3_subtask_cancel(task));
        lookup->status = (task << 4) | state;
        if (state != DREAM_HTTP_P3_SUBTASK_RETURNED && state != DREAM_HTTP_P3_SUBTASK_RETURNED_CANCELLED) {
            // The host may still write to the result area until cancellation completes.
            lookup->next = cancelled;
            cancelled = lookup;
            return;
        }
    }
    free_lookup(lookup);
}

int component_socket_lookup_cleanup(void) {
    for (SocketLookup **cursor = &cancelled; *cursor;) {
        SocketLookup *lookup = *cursor;
        dream_http_p3_subtask_state_t state = poll_lookup(lookup);
        if (state == DREAM_HTTP_P3_SUBTASK_RETURNED || state == DREAM_HTTP_P3_SUBTASK_RETURNED_CANCELLED) {
            *cursor = lookup->next;
            free_lookup(lookup);
        } else {
            cursor = &lookup->next;
        }
    }
    return cancelled != NULL;
}

static int next_ip(SocketLookup *lookup, IpAddress *ip) {
    dream_http_p3_subtask_state_t state = poll_lookup(lookup);
    if (state == DREAM_HTTP_P3_SUBTASK_RETURNED_CANCELLED) { return LOOKUP_FAILED; }
    if (state != DREAM_HTTP_P3_SUBTASK_RETURNED) { return LOOKUP_PENDING; }
    if (lookup->result.is_err) { return LOOKUP_FAILED; }
    if (lookup->index == lookup->result.val.ok.len) { return LOOKUP_END; }
    *ip = lookup->result.val.ok.ptr[lookup->index++];
    return LOOKUP_ADDRESS;
}
#else
void component_socket_lookup_drop(SocketLookup *lookup) {
    if (!lookup) { return; }
    if (!lookup->numeric.ss_family) {
        poll_pollable_drop_own(lookup->ready);
        ip_name_lookup_resolve_address_stream_drop_own(lookup->stream);
        network_network_drop_own(lookup->network);
    }
    free(lookup);
}

int component_socket_lookup_cleanup(void) { return 0; }

static int next_ip(SocketLookup *lookup, IpAddress *ip) {
    if (!poll_method_pollable_ready(poll_borrow_pollable(lookup->ready))) { return LOOKUP_PENDING; }
    ip_name_lookup_option_ip_address_t next;
    ip_name_lookup_error_code_t error;
    if (!ip_name_lookup_method_resolve_address_stream_resolve_next_address(
        ip_name_lookup_borrow_resolve_address_stream(lookup->stream), &next, &error)) {
        return error == NETWORK_ERROR_CODE_WOULD_BLOCK ? LOOKUP_PENDING : LOOKUP_FAILED;
    }
    if (!next.is_some) { return LOOKUP_END; }
    *ip = next.val;
    return LOOKUP_ADDRESS;
}
#endif

int component_socket_lookup_next(SocketLookup *lookup, struct sockaddr_storage *address,
    socklen_t *length) {
    if (lookup->numeric.ss_family) {
        if (!lookup->numeric_size) { return LOOKUP_END; }
        *address = lookup->numeric;
        *length = lookup->numeric_size;
        lookup->numeric_size = 0;
        return LOOKUP_ADDRESS;
    }
    IpAddress ip;
    int state = next_ip(lookup, &ip);
    if (state != LOOKUP_ADDRESS) { return state; }
    if (ip.tag == IPV4) {
        struct sockaddr_in value = {.sin_family = AF_INET, .sin_port = htons(lookup->port)};
        uint8_t octets[] = {ip.val.ipv4.f0, ip.val.ipv4.f1, ip.val.ipv4.f2, ip.val.ipv4.f3};
        memcpy(&value.sin_addr, octets, sizeof(octets));
        memcpy(address, &value, sizeof(value));
        *length = sizeof(value);
    } else {
        struct sockaddr_in6 value = {.sin6_family = AF_INET6, .sin6_port = htons(lookup->port)};
        uint16_t words[] = {
            htons(ip.val.ipv6.f0), htons(ip.val.ipv6.f1), htons(ip.val.ipv6.f2), htons(ip.val.ipv6.f3),
            htons(ip.val.ipv6.f4), htons(ip.val.ipv6.f5), htons(ip.val.ipv6.f6), htons(ip.val.ipv6.f7),
        };
        memcpy(&value.sin6_addr, words, sizeof(words));
        memcpy(address, &value, sizeof(value));
        *length = sizeof(value);
    }
    return LOOKUP_ADDRESS;
}
