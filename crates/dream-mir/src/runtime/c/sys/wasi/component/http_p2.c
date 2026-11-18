#include "http_transport.h"

static struct { int live; wasi_http_types_own_incoming_body_t body; wasi_io_streams_own_input_stream_t stream; } bodies[COMPONENT_LIMIT];
static int next_body = 1;

struct HttpExchange {
    wasi_http_types_own_future_incoming_response_t pending;
    wasi_http_types_own_outgoing_body_t output;
    wasi_io_streams_own_output_stream_t stream;
    wasi_io_poll_own_pollable_t response_ready;
    dream_ptr body;
    size_t written;
    int stream_open, flushing, output_open, waiting_response;
};

HttpExchange *component_http_start(yyjson_val *request, dream_ptr body) {
    wasi_http_types_scheme_t scheme;
    HttpString authority, path;
    if (!dream_component_http_request_target(request, &scheme, &authority, &path)) { return NULL; }
    wasi_http_types_own_fields_t fields = dream_component_http_request_headers(request);
    if (fields.__handle < 0) { return NULL; }
    wasi_http_types_own_outgoing_request_t outgoing = wasi_http_types_constructor_outgoing_request(fields);
    wasi_http_types_borrow_outgoing_request_t borrow = wasi_http_types_borrow_outgoing_request(outgoing);
    wasi_http_types_method_t method = {.tag = WASI_HTTP_TYPES_METHOD_OTHER};
    method.val.other = dream_component_http_string(yyjson_get_str(yyjson_obj_get(request, "method")));
    int valid = wasi_http_types_method_outgoing_request_set_method(borrow, &method)
        && wasi_http_types_method_outgoing_request_set_scheme(borrow, &scheme)
        && wasi_http_types_method_outgoing_request_set_authority(borrow, &authority)
        && wasi_http_types_method_outgoing_request_set_path_with_query(borrow, &path);
    wasi_http_types_own_outgoing_body_t output;
    if (!valid || !wasi_http_types_method_outgoing_request_body(borrow, &output)) {
        wasi_http_types_outgoing_request_drop_own(outgoing); return NULL;
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
    if (!sent) { wasi_http_types_outgoing_body_drop_own(output); wasi_http_types_error_code_free(&error); return NULL; }
    HttpExchange *exchange = calloc(1, sizeof(*exchange));
    if (!exchange) {
        wasi_http_types_outgoing_body_drop_own(output);
        wasi_http_types_future_incoming_response_drop_own(pending);
        return NULL;
    }
    exchange->pending = pending;
    exchange->output = output;
    exchange->output_open = 1;
    exchange->body = body;
    dream_retain(body);
    if (body && dream_i32(body)[0]) {
        if (!wasi_http_types_method_outgoing_body_write(wasi_http_types_borrow_outgoing_body(output), &exchange->stream)) {
            component_http_drop(exchange);
            return NULL;
        }
        exchange->stream_open = 1;
    }
    return exchange;
}

static int finish_upload(HttpExchange *exchange) {
    if (exchange->stream_open) {
        wasi_io_streams_borrow_output_stream_t stream = wasi_io_streams_borrow_output_stream(exchange->stream);
        uint64_t capacity;
        wasi_io_streams_stream_error_t failure;
        if (!wasi_io_streams_method_output_stream_check_write(stream, &capacity, &failure)) {
            wasi_io_streams_stream_error_free(&failure);
            return HTTP_FAILED;
        }
        if (!capacity) { return HTTP_PENDING; }
        if (!exchange->flushing) {
            size_t length = (size_t)dream_i32(exchange->body)[0] - exchange->written;
            size_t count = capacity < length ? (size_t)capacity : length;
            dream_http_p2_list_u8_t bytes = {dream_array_bytes(exchange->body) + exchange->written, count};
            if (!wasi_io_streams_method_output_stream_write(stream, &bytes, &failure)) {
                wasi_io_streams_stream_error_free(&failure);
                return HTTP_FAILED;
            }
            exchange->written += count;
            if (count < length) { return HTTP_PENDING; }
            if (!wasi_io_streams_method_output_stream_flush(stream, &failure)) {
                wasi_io_streams_stream_error_free(&failure);
                return HTTP_FAILED;
            }
            exchange->flushing = 1;
            return HTTP_PENDING;
        }
        wasi_io_streams_output_stream_drop_own(exchange->stream);
        exchange->stream_open = 0;
    }
    wasi_http_types_error_code_t error;
    exchange->output_open = 0;
    if (!wasi_http_types_static_outgoing_body_finish(exchange->output, NULL, &error)) {
        wasi_http_types_error_code_free(&error);
        return HTTP_FAILED;
    }
    exchange->response_ready = wasi_http_types_method_future_incoming_response_subscribe(
        wasi_http_types_borrow_future_incoming_response(exchange->pending));
    exchange->waiting_response = 1;
    return 0;
}

int component_http_poll(HttpExchange *exchange, int *status,
    yyjson_mut_doc *metadata, yyjson_mut_val *headers) {
    if (exchange->output_open) {
        int uploaded = finish_upload(exchange);
        if (uploaded < 0) { return uploaded; }
    }
    if (!wasi_io_poll_method_pollable_ready(wasi_io_poll_borrow_pollable(exchange->response_ready))) {
        return HTTP_PENDING;
    }
    wasi_http_types_result_result_own_incoming_response_error_code_void_t response;
    int ready = wasi_http_types_method_future_incoming_response_get(wasi_http_types_borrow_future_incoming_response(exchange->pending), &response);
    if (!ready || response.is_err || response.val.ok.is_err) {
        if (ready) { wasi_http_types_result_result_own_incoming_response_error_code_void_free(&response); }
        return HTTP_FAILED;
    }
    wasi_http_types_own_incoming_response_t incoming = response.val.ok.val.ok;
    *status = wasi_http_types_method_incoming_response_status(wasi_http_types_borrow_incoming_response(incoming));
    dream_component_http_response_headers(wasi_http_types_method_incoming_response_headers(wasi_http_types_borrow_incoming_response(incoming)), metadata, headers);
    wasi_http_types_own_incoming_body_t input;
    int valid = wasi_http_types_method_incoming_response_consume(wasi_http_types_borrow_incoming_response(incoming), &input);
    wasi_http_types_incoming_response_drop_own(incoming);
    if (!valid) { return HTTP_FAILED; }
    wasi_io_streams_own_input_stream_t stream;
    if (!wasi_http_types_method_incoming_body_stream(wasi_http_types_borrow_incoming_body(input), &stream)) { wasi_http_types_incoming_body_drop_own(input); return HTTP_FAILED; }
    if (next_body == COMPONENT_LIMIT) { wasi_io_streams_input_stream_drop_own(stream); wasi_http_types_incoming_body_drop_own(input); return HTTP_FAILED; }
    int id = next_body++;
    bodies[id].live = 1; bodies[id].body = input; bodies[id].stream = stream;
    return id;
}

void component_http_drop(HttpExchange *exchange) {
    if (!exchange) { return; }
    if (exchange->stream_open) { wasi_io_streams_output_stream_drop_own(exchange->stream); }
    if (exchange->output_open) { wasi_http_types_outgoing_body_drop_own(exchange->output); }
    if (exchange->waiting_response) { wasi_io_poll_pollable_drop_own(exchange->response_ready); }
    wasi_http_types_future_incoming_response_drop_own(exchange->pending);
    dream_release(exchange->body);
    free(exchange);
}

int component_http_poll_cleanup(void) { return 0; }

HttpRead *component_http_read_start(int body, size_t length) {
    if (body <= 0 || body >= COMPONENT_LIMIT || !bodies[body].live) { return NULL; }
    return dream_component_http_new_read(body, length);
}

int component_http_read_poll(HttpRead *read) {
    if (!read->length) { return 0; }
    int id = read->body;
    dream_http_p2_list_u8_t result;
    wasi_io_streams_stream_error_t error;
    if (!wasi_io_streams_method_input_stream_read(wasi_io_streams_borrow_input_stream(bodies[id].stream), read->length, &result, &error)) {
        int closed = error.tag == WASI_IO_STREAMS_STREAM_ERROR_CLOSED;
        wasi_io_streams_stream_error_free(&error);
        return closed ? 0 : HTTP_FAILED;
    }
    int count = result.len ? (int)result.len : HTTP_PENDING;
    if (result.len) { memcpy(read->bytes, result.ptr, result.len); }
    dream_http_p2_list_u8_free(&result);
    return count;
}

void component_http_read_drop(HttpRead *read) { free(read); }

void component_http_close(int id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !bodies[id].live) { return; }
    bodies[id].live = 0;
    wasi_io_streams_input_stream_drop_own(bodies[id].stream);
    wasi_http_types_incoming_body_drop_own(bodies[id].body);
}
