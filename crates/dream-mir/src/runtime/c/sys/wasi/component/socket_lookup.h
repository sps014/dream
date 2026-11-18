#ifndef DREAM_COMPONENT_SOCKET_LOOKUP_H
#define DREAM_COMPONENT_SOCKET_LOOKUP_H
#include <stdint.h>
#include <sys/socket.h>

typedef struct SocketLookup SocketLookup;

enum { LOOKUP_END = 0, LOOKUP_ADDRESS = 1, LOOKUP_PENDING = -1, LOOKUP_FAILED = -2 };

SocketLookup *component_socket_lookup_start(const char *host, uint16_t port);
int component_socket_lookup_next(SocketLookup *lookup, struct sockaddr_storage *address,
    socklen_t *length);
void component_socket_lookup_drop(SocketLookup *lookup);
int component_socket_lookup_cleanup(void);
#endif
