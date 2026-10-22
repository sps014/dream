#ifndef DREAM_COMPONENT_NETWORK_H
#define DREAM_COMPONENT_NETWORK_H
#include "dream_host_support.h"
#include "yyjson.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#define COMPONENT_LIMIT 1024

dream_ptr component_bytes(const void *bytes, size_t length);
dream_ptr component_json(yyjson_mut_doc *doc, int status);
dream_ptr component_error(const char *code, const char *message);

int component_http_send(yyjson_val *request, dream_ptr body, int *status,
    yyjson_mut_doc *metadata, yyjson_mut_val *headers);
int component_http_read(int body, uint8_t *bytes, size_t length);
void component_http_close(int body);
#endif
