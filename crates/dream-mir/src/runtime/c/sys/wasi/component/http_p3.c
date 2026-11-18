#include "http_transport.h"

static struct {
    int live, eof;
    wasi_http_types_stream_u8_t stream;
    wasi_http_types_future_result_option_own_trailers_error_code_t trailers;
    wasi_http_types_future_result_void_error_code_writer_t completion;
} bodies[COMPONENT_LIMIT];
static int next_body = 1;

struct HttpExchange {
    HttpExchange *next;
    wasi_http_types_stream_u8_writer_t writer;
    wasi_http_types_future_result_option_own_trailers_error_code_writer_t trailers;
    wasi_http_types_future_result_void_error_code_t completion;
    wasi_http_client_result_own_response_error_code_t response;
    wasi_http_types_result_option_own_trailers_error_code_t no_trailers;
    dream_ptr body;
    size_t written;
    uint32_t task, write_status, trailers_status;
    uint32_t waitable;
    dream_http_p3_waitable_set_t set;
    int write_pending, upload_done, trailers_done, response_consumed;
};

static HttpExchange *cancelled_exchanges;

static void stop_wait(uint32_t *waiting, dream_http_p3_waitable_set_t *set) {
    if (*waiting) {
        dream_http_p3_waitable_join(*waiting, 0);
        dream_http_p3_waitable_set_drop(*set);
        *waiting = 0;
    }
}

static int poll_wait(uint32_t handle, uint32_t *waiting, dream_http_p3_waitable_set_t *set,
    dream_http_p3_event_t *event) {
    if (!*waiting) {
        *set = dream_http_p3_waitable_set_new();
        dream_http_p3_waitable_join(handle, *set);
        *waiting = handle;
    }
    dream_http_p3_waitable_set_poll(*set, event);
    if (event->event == DREAM_HTTP_P3_EVENT_NONE) { return 0; }
    stop_wait(waiting, set);
    return 1;
}

HttpExchange *component_http_start(yyjson_val *request, dream_ptr body) {
    wasi_http_types_scheme_t scheme;
    HttpString authority, path;
    if (!dream_component_http_request_target(request, &scheme, &authority, &path)) { return NULL; }
    wasi_http_types_own_fields_t fields = dream_component_http_request_headers(request);
    if (fields.__handle < 0) { return NULL; }
    wasi_http_types_stream_u8_writer_t writer;
    wasi_http_types_stream_u8_t contents = wasi_http_types_stream_u8_new(&writer);
    wasi_http_types_future_result_option_own_trailers_error_code_writer_t trailers_writer;
    wasi_http_types_future_result_option_own_trailers_error_code_t trailers = wasi_http_types_future_result_option_own_trailers_error_code_new(&trailers_writer);
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
    method.val.other = dream_component_http_string(yyjson_get_str(yyjson_obj_get(request, "method")));
    int valid = wasi_http_types_method_request_set_method(borrow, &method)
        && wasi_http_types_method_request_set_scheme(borrow, &scheme)
        && wasi_http_types_method_request_set_authority(borrow, &authority)
        && wasi_http_types_method_request_set_path_with_query(borrow, &path);
    if (!valid) { wasi_http_types_request_drop_own(outgoing.f0); wasi_http_types_stream_u8_drop_writable(writer); wasi_http_types_future_result_void_error_code_drop_readable(outgoing.f1); wasi_http_types_future_result_option_own_trailers_error_code_drop_writable(trailers_writer); return NULL; }
    HttpExchange *exchange = calloc(1, sizeof(*exchange));
    if (!exchange) {
        wasi_http_types_request_drop_own(outgoing.f0);
        wasi_http_types_stream_u8_drop_writable(writer);
        wasi_http_types_future_result_void_error_code_drop_readable(outgoing.f1);
        wasi_http_types_future_result_option_own_trailers_error_code_drop_writable(trailers_writer);
        return NULL;
    }
    exchange->body = body;
    dream_retain(body);
    exchange->writer = writer;
    exchange->trailers = trailers_writer;
    exchange->completion = outgoing.f1;
    exchange->trailers_status = wasi_http_types_future_result_option_own_trailers_error_code_write(
        trailers_writer, &exchange->no_trailers);
    exchange->task = wasi_http_client_send(outgoing.f0, &exchange->response);
    return exchange;
}

static int finish_upload(HttpExchange *exchange) {
    size_t length = exchange->body ? (size_t)dream_i32(exchange->body)[0] : 0;
    while (exchange->written < length) {
        if (!exchange->write_pending) {
            exchange->write_status = wasi_http_types_stream_u8_write(exchange->writer,
                dream_array_bytes(exchange->body) + exchange->written, length - exchange->written);
        }
        if (exchange->write_status == DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) {
            exchange->write_pending = 1;
            dream_http_p3_event_t event;
            if (!poll_wait(exchange->writer, &exchange->waitable, &exchange->set, &event)) {
                return HTTP_PENDING;
            }
            exchange->write_status = event.code;
            exchange->write_pending = 0;
        }
        if (DREAM_HTTP_P3_WAITABLE_STATE(exchange->write_status) != DREAM_HTTP_P3_WAITABLE_COMPLETED) { break; }
        exchange->written += DREAM_HTTP_P3_WAITABLE_COUNT(exchange->write_status);
    }
    wasi_http_types_stream_u8_drop_writable(exchange->writer);
    exchange->upload_done = 1;
    return 0;
}

int component_http_poll(HttpExchange *exchange, int *status,
    yyjson_mut_doc *metadata, yyjson_mut_val *headers) {
    if (!exchange->upload_done && finish_upload(exchange) == HTTP_PENDING) { return HTTP_PENDING; }
    if (!exchange->trailers_done) {
        if (exchange->trailers_status == DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) {
            dream_http_p3_event_t event;
            if (!poll_wait(exchange->trailers, &exchange->waitable, &exchange->set, &event)) { return HTTP_PENDING; }
        }
        wasi_http_types_future_result_option_own_trailers_error_code_drop_writable(exchange->trailers);
        exchange->trailers_done = 1;
    }
    uint32_t handle = DREAM_HTTP_P3_SUBTASK_HANDLE(exchange->task);
    if (DREAM_HTTP_P3_SUBTASK_STATE(exchange->task) != DREAM_HTTP_P3_SUBTASK_RETURNED) {
        dream_http_p3_event_t event;
        if (!poll_wait(handle, &exchange->waitable, &exchange->set, &event)) { return HTTP_PENDING; }
        exchange->task = (handle << 4) | event.code;
        if (event.code < DREAM_HTTP_P3_SUBTASK_RETURNED) { return HTTP_PENDING; }
        if (event.code != DREAM_HTTP_P3_SUBTASK_RETURNED) { return HTTP_FAILED; }
    }
    exchange->response_consumed = 1;
    if (exchange->response.is_err) {
        wasi_http_client_result_own_response_error_code_free(&exchange->response);
        return HTTP_FAILED;
    }
    wasi_http_types_own_response_t incoming = exchange->response.val.ok;
    *status = wasi_http_types_method_response_get_status_code(wasi_http_types_borrow_response(incoming));
    dream_component_http_response_headers(wasi_http_types_method_response_get_headers(wasi_http_types_borrow_response(incoming)), metadata, headers);
    wasi_http_types_future_result_void_error_code_writer_t completion;
    wasi_http_types_future_result_void_error_code_t result = wasi_http_types_future_result_void_error_code_new(&completion);
    wasi_http_types_tuple2_stream_u8_future_result_option_own_trailers_error_code_t input;
    wasi_http_types_static_response_consume_body(incoming, result, &input);
    if (next_body == COMPONENT_LIMIT) {
        wasi_http_types_stream_u8_drop_readable(input.f0);
        wasi_http_types_future_result_option_own_trailers_error_code_drop_readable(input.f1);
        wasi_http_types_future_result_void_error_code_drop_writable(completion);
        return HTTP_FAILED;
    }
    int id = next_body++;
    bodies[id].live = 1; bodies[id].stream = input.f0; bodies[id].trailers = input.f1; bodies[id].completion = completion;
    return id;
}

static void free_exchange(HttpExchange *exchange) {
    uint32_t state = DREAM_HTTP_P3_SUBTASK_STATE(exchange->task);
    uint32_t handle = DREAM_HTTP_P3_SUBTASK_HANDLE(exchange->task);
    if (handle) { dream_http_p3_subtask_drop(handle); }
    if (!exchange->response_consumed && state == DREAM_HTTP_P3_SUBTASK_RETURNED) {
        if (exchange->response.is_err) { wasi_http_client_result_own_response_error_code_free(&exchange->response); }
        else { wasi_http_types_response_drop_own(exchange->response.val.ok); }
    }
    wasi_http_types_future_result_void_error_code_drop_readable(exchange->completion);
    dream_release(exchange->body);
    free(exchange);
}

void component_http_drop(HttpExchange *exchange) {
    if (!exchange) { return; }
    stop_wait(&exchange->waitable, &exchange->set);
    if (!exchange->upload_done) {
        if (exchange->write_pending) { wasi_http_types_stream_u8_cancel_write(exchange->writer); }
        wasi_http_types_stream_u8_drop_writable(exchange->writer);
    }
    if (!exchange->trailers_done) {
        if (exchange->trailers_status == DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) {
            wasi_http_types_future_result_option_own_trailers_error_code_cancel_write(exchange->trailers);
        }
        wasi_http_types_future_result_option_own_trailers_error_code_drop_writable(exchange->trailers);
    }
    uint32_t handle = DREAM_HTTP_P3_SUBTASK_HANDLE(exchange->task);
    uint32_t state = DREAM_HTTP_P3_SUBTASK_STATE(exchange->task);
    if (handle && state < DREAM_HTTP_P3_SUBTASK_RETURNED) {
        state = DREAM_HTTP_P3_SUBTASK_STATE(dream_http_p3_subtask_cancel(handle));
        exchange->task = (handle << 4) | state;
        if (state != DREAM_HTTP_P3_SUBTASK_RETURNED
            && state != DREAM_HTTP_P3_SUBTASK_RETURNED_CANCELLED) {
            exchange->next = cancelled_exchanges;
            cancelled_exchanges = exchange;
            return;
        }
    }
    free_exchange(exchange);
}

int component_http_poll_cleanup(void) {
    for (HttpExchange **cursor = &cancelled_exchanges; *cursor;) {
        HttpExchange *exchange = *cursor;
        uint32_t handle = DREAM_HTTP_P3_SUBTASK_HANDLE(exchange->task);
        dream_http_p3_event_t event;
        if (poll_wait(handle, &exchange->waitable, &exchange->set, &event)
            && (event.code == DREAM_HTTP_P3_SUBTASK_RETURNED
                || event.code == DREAM_HTTP_P3_SUBTASK_RETURNED_CANCELLED)) {
            *cursor = exchange->next;
            exchange->task = (handle << 4) | event.code;
            free_exchange(exchange);
        } else {
            cursor = &exchange->next;
        }
    }
    return cancelled_exchanges != NULL;
}

HttpRead *component_http_read_start(int body, size_t length) {
    if (body <= 0 || body >= COMPONENT_LIMIT || !bodies[body].live) { return NULL; }
    return dream_component_http_new_read(body, length);
}

int component_http_read_poll(HttpRead *read) {
    int id = read->body;
    if (!read->length || (bodies[id].eof && !read->at_trailers)) { return 0; }
    uint32_t result;
    dream_http_p3_event_t event;
    if (!read->at_trailers) {
        uint32_t handle = bodies[id].stream;
        result = read->waitable ? DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED
            : wasi_http_types_stream_u8_read(handle, read->bytes, read->length);
        if (result == DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) {
            if (!poll_wait(handle, &read->waitable, &read->set, &event)) { return HTTP_PENDING; }
            result = event.code;
        }
        switch (DREAM_HTTP_P3_WAITABLE_STATE(result)) {
            case DREAM_HTTP_P3_WAITABLE_COMPLETED: return (int)DREAM_HTTP_P3_WAITABLE_COUNT(result);
            case DREAM_HTTP_P3_WAITABLE_DROPPED: break;
            default: return HTTP_FAILED;
        }
        bodies[id].eof = 1;
    }
    uint32_t handle = bodies[id].trailers;
    result = read->at_trailers ? DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED
        : wasi_http_types_future_result_option_own_trailers_error_code_read(handle, &read->trailers);
    read->at_trailers = 1;
    if (result == DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) {
        if (!poll_wait(handle, &read->waitable, &read->set, &event)) { return HTTP_PENDING; }
        result = event.code;
    }
    if (DREAM_HTTP_P3_WAITABLE_STATE(result) != DREAM_HTTP_P3_WAITABLE_COMPLETED) { return HTTP_FAILED; }
    int failed = read->trailers.is_err;
    if (!failed && read->trailers.val.ok.is_some) {
        wasi_http_types_fields_drop_own(read->trailers.val.ok.val);
    } else if (failed) {
        wasi_http_types_error_code_free(&read->trailers.val.err);
    }
    return failed ? HTTP_FAILED : 0;
}

void component_http_read_drop(HttpRead *read) {
    if (!read) { return; }
    if (read->waitable) {
        uint32_t handle = read->waitable;
        stop_wait(&read->waitable, &read->set);
        if (read->at_trailers) { wasi_http_types_future_result_option_own_trailers_error_code_cancel_read(handle); }
        else { wasi_http_types_stream_u8_cancel_read(handle); }
    }
    free(read);
}

void component_http_close(int id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !bodies[id].live) { return; }
    bodies[id].live = 0;
    wasi_http_types_stream_u8_drop_readable(bodies[id].stream);
    wasi_http_types_future_result_option_own_trailers_error_code_drop_readable(bodies[id].trailers);
    wasi_http_types_result_void_error_code_t completed = {0};
    uint32_t state = wasi_http_types_future_result_void_error_code_write(bodies[id].completion, &completed);
    if (state == DREAM_HTTP_P3_WAITABLE_STATUS_BLOCKED) {
        wasi_http_types_future_result_void_error_code_cancel_write(bodies[id].completion);
    }
    wasi_http_types_future_result_void_error_code_drop_writable(bodies[id].completion);
}
