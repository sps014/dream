#include "http_transport.h"

HttpString dream_component_http_string(const char *text) {
    return (HttpString){(uint8_t *)(text ? text : ""), text ? strlen(text) : 0};
}

int dream_component_http_request_target(yyjson_val *request, wasi_http_types_scheme_t *scheme,
    HttpString *authority, HttpString *path) {
    const char *name = yyjson_get_str(yyjson_obj_get(request, "scheme"));
    const char *host = yyjson_get_str(yyjson_obj_get(request, "authority"));
    const char *target = yyjson_get_str(yyjson_obj_get(request, "target"));
    if (!name || !host || !target) { return 0; }
    scheme->tag = !strcmp(name, "https") ? WASI_HTTP_TYPES_SCHEME_HTTPS : WASI_HTTP_TYPES_SCHEME_HTTP;
    *authority = dream_component_http_string(host);
    *path = dream_component_http_string(target);
    return 1;
}

wasi_http_types_own_fields_t dream_component_http_request_headers(yyjson_val *request) {
    wasi_http_types_own_fields_t fields = wasi_http_types_constructor_fields();
    yyjson_val *headers = yyjson_obj_get(request, "headers"), *item;
    size_t index, max;
    yyjson_arr_foreach(headers, index, max, item) {
        HttpString name = dream_component_http_string(yyjson_get_str(yyjson_arr_get(item, 0)));
        const char *text = yyjson_get_str(yyjson_arr_get(item, 1));
        wasi_http_types_field_value_t value = {(uint8_t *)(text ? text : ""), text ? strlen(text) : 0};
        wasi_http_types_header_error_t error;
        if (!wasi_http_types_method_fields_append(wasi_http_types_borrow_fields(fields), &name, &value, &error)) {
            wasi_http_types_fields_drop_own(fields); return (wasi_http_types_own_fields_t){-1};
        }
    }
    return fields;
}

void dream_component_http_response_headers(wasi_http_types_own_fields_t fields, yyjson_mut_doc *doc, yyjson_mut_val *headers) {
    wasi_http_types_list_tuple2_field_name_field_value_t entries;
#ifdef __wasip3__
    wasi_http_types_method_fields_copy_all(wasi_http_types_borrow_fields(fields), &entries);
#else
    wasi_http_types_method_fields_entries(wasi_http_types_borrow_fields(fields), &entries);
#endif
    for (size_t i = 0; i < entries.len; i++) {
        yyjson_mut_val *pair = yyjson_mut_arr(doc);
        yyjson_mut_arr_add_strncpy(doc, pair, (char *)entries.ptr[i].f0.ptr, entries.ptr[i].f0.len);
        yyjson_mut_arr_add_strncpy(doc, pair, (char *)entries.ptr[i].f1.ptr, entries.ptr[i].f1.len);
        yyjson_mut_arr_add_val(headers, pair);
    }
    wasi_http_types_list_tuple2_field_name_field_value_free(&entries);
    wasi_http_types_fields_drop_own(fields);
}

const uint8_t *component_http_read_bytes(const HttpRead *read) { return read->bytes; }

HttpRead *dream_component_http_new_read(int body, size_t length) {
    HttpRead *read = calloc(1, sizeof(*read) + length);
    if (read) { read->body = body; read->length = length; }
    return read;
}
