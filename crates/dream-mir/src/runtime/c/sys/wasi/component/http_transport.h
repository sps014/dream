#ifndef DREAM_COMPONENT_HTTP_TRANSPORT_H
#define DREAM_COMPONENT_HTTP_TRANSPORT_H
#include "network.h"
#if defined(__wasip3__)
#include "p3/dream_http_p3.h"
typedef dream_http_p3_string_t HttpString;
#else
#include "p2/dream_http_p2.h"
typedef dream_http_p2_string_t HttpString;
#endif

struct HttpRead {
    int body;
    size_t length;
#ifdef __wasip3__
    uint32_t waitable;
    dream_http_p3_waitable_set_t set;
    int at_trailers;
    wasi_http_types_result_option_own_trailers_error_code_t trailers;
#endif
    uint8_t bytes[];
};

HttpString dream_component_http_string(const char *text);
int dream_component_http_request_target(yyjson_val *request, wasi_http_types_scheme_t *scheme,
    HttpString *authority, HttpString *path);
wasi_http_types_own_fields_t dream_component_http_request_headers(yyjson_val *request);
void dream_component_http_response_headers(wasi_http_types_own_fields_t fields,
    yyjson_mut_doc *doc, yyjson_mut_val *headers);
HttpRead *dream_component_http_new_read(int body, size_t length);
#endif
