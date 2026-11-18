#ifndef DREAM_COMPONENT_NETWORK_H
#define DREAM_COMPONENT_NETWORK_H
#include "dream_host_support.h"
#include "yyjson.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#define COMPONENT_LIMIT 1024

void dream_component_register_http(int (*poll)(void));
void dream_component_register_sockets(int (*poll)(void));

dream_ptr component_bytes(const void *bytes, size_t length);
dream_ptr component_copy_bytes(dream_ptr bytes);
dream_ptr component_json(yyjson_mut_doc *doc, int status);
dream_ptr component_error(const char *code, const char *message);

typedef struct HttpExchange HttpExchange;
typedef struct HttpRead HttpRead;

enum { HTTP_FAILED = -1, HTTP_PENDING = -2 };

HttpExchange *component_http_start(yyjson_val *request, dream_ptr body);
int component_http_poll(HttpExchange *exchange, int *status,
    yyjson_mut_doc *metadata, yyjson_mut_val *headers);
void component_http_drop(HttpExchange *exchange);
int component_http_poll_cleanup(void);

HttpRead *component_http_read_start(int body, size_t length);
int component_http_read_poll(HttpRead *read);
const uint8_t *component_http_read_bytes(const HttpRead *read);
void component_http_read_drop(HttpRead *read);
void component_http_close(int body);
#endif
