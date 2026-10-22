#include "network.h"
#if defined(__wasip3__)
#include "p3/dream_http_p3.h"
typedef dream_http_p3_string_t HttpString;
#else
#include "p2/dream_http_p2.h"
typedef dream_http_p2_string_t HttpString;
#endif

static HttpString http_string(const char *text) {
    return (HttpString){(uint8_t *)(text ? text : ""), text ? strlen(text) : 0};
}

static int request_target(yyjson_val *request, wasi_http_types_scheme_t *scheme,
    HttpString *authority, HttpString *path) {
    const char *name = yyjson_get_str(yyjson_obj_get(request, "scheme"));
    const char *host = yyjson_get_str(yyjson_obj_get(request, "authority"));
    const char *target = yyjson_get_str(yyjson_obj_get(request, "target"));
    if (!name || !host || !target) { return 0; }
    scheme->tag = !strcmp(name, "https") ? WASI_HTTP_TYPES_SCHEME_HTTPS : WASI_HTTP_TYPES_SCHEME_HTTP;
    *authority = http_string(host);
    *path = http_string(target);
    return 1;
}

static wasi_http_types_own_fields_t request_headers(yyjson_val *request) {
    wasi_http_types_own_fields_t fields = wasi_http_types_constructor_fields();
    yyjson_val *headers = yyjson_obj_get(request, "headers"), *item;
    size_t index, max;
    yyjson_arr_foreach(headers, index, max, item) {
        HttpString name = http_string(yyjson_get_str(yyjson_arr_get(item, 0)));
        const char *text = yyjson_get_str(yyjson_arr_get(item, 1));
        wasi_http_types_field_value_t value = {(uint8_t *)(text ? text : ""), text ? strlen(text) : 0};
        wasi_http_types_header_error_t error;
        if (!wasi_http_types_method_fields_append(wasi_http_types_borrow_fields(fields), &name, &value, &error)) {
            wasi_http_types_fields_drop_own(fields); return (wasi_http_types_own_fields_t){-1};
        }
    }
    return fields;
}

static void response_headers(wasi_http_types_own_fields_t fields, yyjson_mut_doc *doc, yyjson_mut_val *headers) {
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

#ifndef __wasip3__
static struct { int live; wasi_http_types_own_incoming_body_t body; wasi_io_streams_own_input_stream_t stream; } bodies[COMPONENT_LIMIT];
static int next_body = 1;

int component_http_send(yyjson_val *request, dream_ptr body, int *status,
    yyjson_mut_doc *metadata, yyjson_mut_val *headers) {
    wasi_http_types_scheme_t scheme;
    HttpString authority, path;
    if (!request_target(request, &scheme, &authority, &path)) { return 0; }
    wasi_http_types_own_fields_t fields = request_headers(request);
    if (fields.__handle < 0) { return 0; }
    wasi_http_types_own_outgoing_request_t outgoing = wasi_http_types_constructor_outgoing_request(fields);
    wasi_http_types_borrow_outgoing_request_t borrow = wasi_http_types_borrow_outgoing_request(outgoing);
    wasi_http_types_method_t method = {.tag = WASI_HTTP_TYPES_METHOD_OTHER};
    method.val.other = http_string(yyjson_get_str(yyjson_obj_get(request, "method")));
    int valid = wasi_http_types_method_outgoing_request_set_method(borrow, &method)
        && wasi_http_types_method_outgoing_request_set_scheme(borrow, &scheme)
        && wasi_http_types_method_outgoing_request_set_authority(borrow, &authority)
        && wasi_http_types_method_outgoing_request_set_path_with_query(borrow, &path);
    wasi_http_types_own_outgoing_body_t output;
    if (!valid || !wasi_http_types_method_outgoing_request_body(borrow, &output)) {
        wasi_http_types_outgoing_request_drop_own(outgoing); return 0;
    }
    wasi_http_types_own_request_options_t options = wasi_http_types_constructor_request_options();
    uint64_t timeout = yyjson_get_uint(yyjson_obj_get(request, "timeout_ms")) * 1000000;
    if (timeout) {
        wasi_http_types_method_request_options_set_first_byte_timeout(wasi_http_types_borrow_request_options(options), &timeout);
        wasi_http_types_method_request_options_set_between_bytes_timeout(wasi_http_types_borrow_request_options(options), &timeout);
    }
    uint64_t connect = yyjson_get_uint(yyjson_obj_get(request, "connect_timeout_ms")) * 1000000;
    if (connect) { wasi_http_types_method_request_options_set_connect_timeout(wasi_http_types_borrow_request_options(options), &connect); }
    wasi_http_types_own_future_incoming_response_t pending;
    wasi_http_types_error_code_t error;
    int sent = wasi_http_outgoing_handler_handle(outgoing, &options, &pending, &error);
    if (!sent) { wasi_http_types_outgoing_body_drop_own(output); wasi_http_types_error_code_free(&error); return 0; }
    int32_t length = body ? dream_i32(body)[0] : 0;
    if (length) {
        wasi_io_streams_own_output_stream_t stream;
        if (!wasi_http_types_method_outgoing_body_write(wasi_http_types_borrow_outgoing_body(output), &stream)) { wasi_http_types_outgoing_body_drop_own(output); wasi_http_types_future_incoming_response_drop_own(pending); return 0; }
        dream_http_p2_list_u8_t bytes = {(uint8_t *)dream_p(body) + 4, (size_t)length};
        wasi_io_streams_stream_error_t failure;
        valid = wasi_io_streams_method_output_stream_blocking_write_and_flush(wasi_io_streams_borrow_output_stream(stream), &bytes, &failure);
        wasi_io_streams_output_stream_drop_own(stream);
        if (!valid) { wasi_io_streams_stream_error_free(&failure); }
    }
    if (!wasi_http_types_static_outgoing_body_finish(output, NULL, &error)) { wasi_http_types_error_code_free(&error); }
    wasi_io_poll_own_pollable_t poll = wasi_http_types_method_future_incoming_response_subscribe(wasi_http_types_borrow_future_incoming_response(pending));
    wasi_io_poll_method_pollable_block(wasi_io_poll_borrow_pollable(poll));
    wasi_io_poll_pollable_drop_own(poll);
    wasi_http_types_result_result_own_incoming_response_error_code_void_t response;
    int ready = wasi_http_types_method_future_incoming_response_get(wasi_http_types_borrow_future_incoming_response(pending), &response);
    wasi_http_types_future_incoming_response_drop_own(pending);
    if (!ready || response.is_err || response.val.ok.is_err) {
        if (ready) { wasi_http_types_result_result_own_incoming_response_error_code_void_free(&response); }
        return 0;
    }
    wasi_http_types_own_incoming_response_t incoming = response.val.ok.val.ok;
    *status = wasi_http_types_method_incoming_response_status(wasi_http_types_borrow_incoming_response(incoming));
    response_headers(wasi_http_types_method_incoming_response_headers(wasi_http_types_borrow_incoming_response(incoming)), metadata, headers);
    wasi_http_types_own_incoming_body_t input;
    valid = wasi_http_types_method_incoming_response_consume(wasi_http_types_borrow_incoming_response(incoming), &input);
    wasi_http_types_incoming_response_drop_own(incoming);
    if (!valid) { return 0; }
    wasi_io_streams_own_input_stream_t stream;
    if (!wasi_http_types_method_incoming_body_stream(wasi_http_types_borrow_incoming_body(input), &stream)) { wasi_http_types_incoming_body_drop_own(input); return 0; }
    if (next_body == COMPONENT_LIMIT) { wasi_io_streams_input_stream_drop_own(stream); wasi_http_types_incoming_body_drop_own(input); return 0; }
    int id = next_body++;
    bodies[id].live = 1; bodies[id].body = input; bodies[id].stream = stream;
    return id;
}

int component_http_read(int id, uint8_t *bytes, size_t length) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !bodies[id].live) { return -1; }
    dream_http_p2_list_u8_t result;
    wasi_io_streams_stream_error_t error;
    if (!wasi_io_streams_method_input_stream_blocking_read(wasi_io_streams_borrow_input_stream(bodies[id].stream), length, &result, &error)) {
        int closed = error.tag == WASI_IO_STREAMS_STREAM_ERROR_CLOSED;
        wasi_io_streams_stream_error_free(&error);
        return closed ? 0 : -1;
    }
    memcpy(bytes, result.ptr, result.len); int count = (int)result.len;
    dream_http_p2_list_u8_free(&result);
    return count;
}

void component_http_close(int id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !bodies[id].live) { return; }
    bodies[id].live = 0;
    wasi_io_streams_input_stream_drop_own(bodies[id].stream);
    wasi_http_types_incoming_body_drop_own(bodies[id].body);
}
#endif

#ifdef __wasip3__
static struct {
    int live, eof;
    wasi_http_types_stream_u8_t stream;
    wasi_http_types_future_result_option_own_trailers_error_code_t trailers;
    wasi_http_types_future_result_void_error_code_writer_t completion;
} bodies[COMPONENT_LIMIT];
static int next_body = 1;

static uint32_t wait_stream(uint32_t handle, uint32_t status) {
    if (status != DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) { return status; }
    dream_http_p3_waitable_set_t set = dream_http_p3_waitable_set_new();
    dream_http_p3_waitable_join(handle, set);
    dream_http_p3_event_t event;
    dream_http_p3_waitable_set_wait(set, &event);
    dream_http_p3_waitable_join(handle, 0);
    dream_http_p3_waitable_set_drop(set);
    return event.code;
}

int component_http_send(yyjson_val *request, dream_ptr body, int *status,
    yyjson_mut_doc *metadata, yyjson_mut_val *headers) {
    wasi_http_types_scheme_t scheme;
    HttpString authority, path;
    if (!request_target(request, &scheme, &authority, &path)) { return 0; }
    wasi_http_types_own_fields_t fields = request_headers(request);
    if (fields.__handle < 0) { return 0; }
    wasi_http_types_stream_u8_writer_t writer;
    wasi_http_types_stream_u8_t contents = wasi_http_types_stream_u8_new(&writer);
    wasi_http_types_future_result_option_own_trailers_error_code_writer_t trailers_writer;
    wasi_http_types_future_result_option_own_trailers_error_code_t trailers = wasi_http_types_future_result_option_own_trailers_error_code_new(&trailers_writer);
    wasi_http_types_result_option_own_trailers_error_code_t no_trailers = {0};
    uint32_t trailers_status = wasi_http_types_future_result_option_own_trailers_error_code_write(trailers_writer, &no_trailers);
    wasi_http_types_own_request_options_t options = wasi_http_types_constructor_request_options();
    wasi_http_types_request_options_error_t option_error;
    uint64_t timeout = yyjson_get_uint(yyjson_obj_get(request, "timeout_ms")) * 1000000;
    if (timeout) {
        wasi_http_types_method_request_options_set_first_byte_timeout(wasi_http_types_borrow_request_options(options), &timeout, &option_error);
        wasi_http_types_method_request_options_set_between_bytes_timeout(wasi_http_types_borrow_request_options(options), &timeout, &option_error);
    }
    uint64_t connect = yyjson_get_uint(yyjson_obj_get(request, "connect_timeout_ms")) * 1000000;
    if (connect) { wasi_http_types_method_request_options_set_connect_timeout(wasi_http_types_borrow_request_options(options), &connect, &option_error); }
    wasi_http_types_tuple2_own_request_future_result_void_error_code_t outgoing;
    wasi_http_types_static_request_new(fields, &contents, trailers, &options, &outgoing);
    wasi_http_types_borrow_request_t borrow = wasi_http_types_borrow_request(outgoing.f0);
    wasi_http_types_method_t method = {.tag = WASI_HTTP_TYPES_METHOD_OTHER};
    method.val.other = http_string(yyjson_get_str(yyjson_obj_get(request, "method")));
    int valid = wasi_http_types_method_request_set_method(borrow, &method)
        && wasi_http_types_method_request_set_scheme(borrow, &scheme)
        && wasi_http_types_method_request_set_authority(borrow, &authority)
        && wasi_http_types_method_request_set_path_with_query(borrow, &path);
    if (!valid) { wasi_http_types_request_drop_own(outgoing.f0); wasi_http_types_stream_u8_drop_writable(writer); wasi_http_types_future_result_void_error_code_drop_readable(outgoing.f1); return 0; }
    wasi_http_client_result_own_response_error_code_t response;
    uint32_t task = wasi_http_client_send(outgoing.f0, &response);
    uint32_t task_handle = DREAM_HTTP_P3_SUBTASK_HANDLE(task);
    int32_t length = body ? dream_i32(body)[0] : 0;
    size_t written = 0;
    while (written < (size_t)length) {
        uint32_t step = wait_stream(writer, wasi_http_types_stream_u8_write(writer, (uint8_t *)dream_p(body) + 4 + written, (size_t)length - written));
        if (DREAM_HTTP_P3_WAITABLE_STATE(step) != DREAM_HTTP_P3_WAITABLE_COMPLETED) { break; }
        written += DREAM_HTTP_P3_WAITABLE_COUNT(step);
    }
    wasi_http_types_stream_u8_drop_writable(writer);
    wait_stream(trailers_writer, trailers_status);
    wasi_http_types_future_result_option_own_trailers_error_code_drop_writable(trailers_writer);
    if (DREAM_HTTP_P3_SUBTASK_STATE(task) != DREAM_HTTP_P3_SUBTASK_RETURNED) {
        dream_http_p3_waitable_set_t set = dream_http_p3_waitable_set_new();
        dream_http_p3_waitable_join(task_handle, set);
        dream_http_p3_event_t event;
        do { dream_http_p3_waitable_set_wait(set, &event); } while (event.code != DREAM_HTTP_P3_SUBTASK_RETURNED);
        dream_http_p3_waitable_join(task_handle, 0); dream_http_p3_waitable_set_drop(set);
    }
    if (task_handle) { dream_http_p3_subtask_drop(task_handle); }
    wasi_http_types_future_result_void_error_code_drop_readable(outgoing.f1);
    if (response.is_err) { wasi_http_client_result_own_response_error_code_free(&response); return 0; }
    wasi_http_types_own_response_t incoming = response.val.ok;
    *status = wasi_http_types_method_response_get_status_code(wasi_http_types_borrow_response(incoming));
    response_headers(wasi_http_types_method_response_get_headers(wasi_http_types_borrow_response(incoming)), metadata, headers);
    wasi_http_types_future_result_void_error_code_writer_t completion;
    wasi_http_types_future_result_void_error_code_t result = wasi_http_types_future_result_void_error_code_new(&completion);
    wasi_http_types_tuple2_stream_u8_future_result_option_own_trailers_error_code_t input;
    wasi_http_types_static_response_consume_body(incoming, result, &input);
    if (next_body == COMPONENT_LIMIT) {
        wasi_http_types_stream_u8_drop_readable(input.f0);
        wasi_http_types_future_result_option_own_trailers_error_code_drop_readable(input.f1);
        wasi_http_types_future_result_void_error_code_drop_writable(completion);
        return 0;
    }
    int id = next_body++;
    bodies[id].live = 1; bodies[id].stream = input.f0; bodies[id].trailers = input.f1; bodies[id].completion = completion;
    return id;
}

int component_http_read(int id, uint8_t *bytes, size_t length) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !bodies[id].live) { return -1; }
    if (!length || bodies[id].eof) { return 0; }
    uint32_t result = wait_stream(bodies[id].stream, wasi_http_types_stream_u8_read(bodies[id].stream, bytes, length));
    if (DREAM_HTTP_P3_WAITABLE_STATE(result) == DREAM_HTTP_P3_WAITABLE_DROPPED) {
        bodies[id].eof = 1;
        wasi_http_types_result_option_own_trailers_error_code_t trailers;
        wait_stream(bodies[id].trailers, wasi_http_types_future_result_option_own_trailers_error_code_read(bodies[id].trailers, &trailers));
        int failed = trailers.is_err;
        wasi_http_types_result_option_own_trailers_error_code_free(&trailers);
        return failed ? -1 : 0;
    }
    return DREAM_HTTP_P3_WAITABLE_STATE(result) == DREAM_HTTP_P3_WAITABLE_COMPLETED ? (int)DREAM_HTTP_P3_WAITABLE_COUNT(result) : -1;
}

void component_http_close(int id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !bodies[id].live) { return; }
    bodies[id].live = 0;
    wasi_http_types_stream_u8_drop_readable(bodies[id].stream);
    wasi_http_types_future_result_option_own_trailers_error_code_drop_readable(bodies[id].trailers);
    wasi_http_types_result_void_error_code_t completed = {0};
    wait_stream(bodies[id].completion, wasi_http_types_future_result_void_error_code_write(bodies[id].completion, &completed));
    wasi_http_types_future_result_void_error_code_drop_writable(bodies[id].completion);
}
#endif
