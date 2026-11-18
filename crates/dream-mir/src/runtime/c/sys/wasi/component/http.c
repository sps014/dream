#include "network.h"

typedef struct {
    int live, child, body, client;
    dream_ptr result, waiter;
    HttpExchange *exchange;
    HttpRead *read;
    yyjson_doc *input;
    yyjson_mut_doc *metadata;
    yyjson_mut_val *headers;
    char *text;
} HttpOperation;

static HttpOperation operations[COMPONENT_LIMIT];
static uint8_t clients[COMPONENT_LIMIT];
static int next_operation = 1, next_client = 1;
static int waiting_count;

dream_ptr component_bytes(const void *bytes, size_t length) {
    if (length > INT32_MAX) { return 0; }
    dream_ptr result = dream_array_new((int32_t)length, 1);
    if (length) { memcpy(dream_array_bytes(result), bytes, length); }
    return result;
}

dream_ptr component_copy_bytes(dream_ptr bytes) {
    if (!bytes || !dream_i32(bytes)[0]) { return 0; }
    return component_bytes(dream_array_bytes(bytes), (size_t)dream_i32(bytes)[0]);
}

dream_ptr component_json(yyjson_mut_doc *doc, int status) {
    size_t length;
    char *json = yyjson_mut_write(doc, 0, &length);
    if (!json || length >= INT32_MAX) { free(json); return 0; }
    dream_ptr result = dream_status_bytes((uint8_t)status, json, length);
    free(json);
    return result;
}

static void put_text(uint8_t *dest, const char *text, size_t length) {
    for (int i = 0; i < 4; i++) { dest[i] = (uint8_t)(length >> (8 * i)); }
    memcpy(dest + 4, text, length);
}

dream_ptr component_error(const char *code, const char *message) {
    size_t code_len = strlen(code), message_len = strlen(message);
    size_t size = 1 + 4 + code_len + 4 + message_len;
    dream_ptr result = dream_array_new((int32_t)size, 1);
    uint8_t *bytes = dream_array_bytes(result);
    bytes[0] = 2;
    put_text(bytes + 1, code, code_len);
    put_text(bytes + 1 + 4 + code_len, message, message_len);
    return result;
}

static int operation(dream_ptr result, int child) {
    if (next_operation == COMPONENT_LIMIT) { dream_release(result); if (child) { component_http_close(child); } return 0; }
    int id = next_operation++;
    operations[id].live = 1; operations[id].result = result; operations[id].child = child;
    return id;
}

int32_t netClientCreate(int32_t cookie_jar) {
    if (cookie_jar > 0) { return 0; }
    if (next_client == COMPONENT_LIMIT) { return 0; }
    int id = next_client++; clients[id] = 1; return id;
}
static void abort_operation(HttpOperation *op, const char *code, const char *message);

void netClientClose(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !clients[id]) { return; }
    clients[id] = 0;
    for (int i = 1; i < next_operation; i++) {
        HttpOperation *op = &operations[i];
        if (op->exchange && op->client == id) {
            abort_operation(op, "ECLOSED", "HTTP client closed during request");
        }
    }
}
int32_t netCapabilities(void) { return 13; }
int32_t netCookieJarCreate(void) { return 0; }
void netCookieJarClose(int32_t id) { (void)id; }
dream_ptr netCookieJarControl(int32_t id, dream_ptr request) {
    (void)id; (void)request;
    return dream_str_from_utf8("{\"code\":\"ENOTSUP\",\"message\":\"managed cookie jars are unavailable on this host\"}");
}

static int poll_http(void);

int32_t netHttpStart(int32_t client, dream_ptr request, dream_ptr body) {
    if (client <= 0 || client >= COMPONENT_LIMIT || !clients[client]) { return operation(component_error("ECLOSED", "HTTP client is closed"), 0); }
    char *text = dream_str_utf8(request);
    yyjson_doc *input = text ? yyjson_read(text, strlen(text), 0) : NULL;
    if (!input) { free(text); return operation(component_error("EINVAL", "invalid HTTP request"), 0); }
    yyjson_val *root = yyjson_doc_get_root(input);
    if (yyjson_get_sint(yyjson_obj_get(root, "max_redirects")) != -1) {
        yyjson_doc_free(input); free(text);
        return operation(component_error("ENOTSUP", "WASI exposes individual responses; redirect limits are unavailable"), 0);
    }
    dream_component_register_http(poll_http);
    dream_ptr payload = component_copy_bytes(body);
    HttpExchange *exchange = component_http_start(root, payload);
    dream_release(payload);
    if (!exchange) {
        yyjson_doc_free(input);
        free(text);
        return operation(component_error("ETRANSPORT", "WASI HTTP request failed"), 0);
    }
    int id = operation(0, 0);
    if (!id) {
        component_http_drop(exchange);
        yyjson_doc_free(input);
        free(text);
        return 0;
    }
    operations[id].exchange = exchange;
    operations[id].client = client;
    operations[id].input = input;
    operations[id].text = text;
    yyjson_mut_doc *metadata = yyjson_mut_doc_new(NULL);
    if (!metadata) {
        component_http_drop(exchange);
        yyjson_doc_free(input);
        free(text);
        operations[id] = (HttpOperation){0};
        return 0;
    }
    yyjson_mut_doc_set_root(metadata, yyjson_mut_obj(metadata));
    operations[id].metadata = metadata;
    operations[id].headers = yyjson_mut_arr(metadata);
    return id;
}

int32_t netBodyRead(int32_t body, int32_t count) {
    if (count < 0 || count > 16 * 1024 * 1024) {
        return operation(component_error("EINVAL", "invalid body read size"), 0);
    }
    for (int i = 1; i < next_operation; i++) {
        if (operations[i].read && operations[i].body == body) {
            return operation(component_error("EBUSY", "HTTP body already has a pending read"), 0);
        }
    }
    HttpRead *read = component_http_read_start(body, (size_t)count);
    if (!read) { return operation(component_error("ECLOSED", "HTTP body is closed"), 0); }
    int id = operation(0, 0);
    if (!id) { component_http_read_drop(read); return 0; }
    operations[id].body = body;
    operations[id].read = read;
    return id;
}

static void clear_transport(HttpOperation *op) {
    component_http_drop(op->exchange);
    component_http_read_drop(op->read);
    op->exchange = NULL;
    op->read = NULL;
    yyjson_doc_free(op->input);
    free(op->text);
    op->input = NULL;
    op->text = NULL;
    yyjson_mut_doc_free(op->metadata);
    op->metadata = NULL;
}

static void wake_waiter(HttpOperation *op) {
    dream_ptr future = op->waiter;
    if (future) {
        op->waiter = 0;
        waiting_count--;
        dream_async_complete(future, 0);
        dream_release(future);
    }
}

static void abort_operation(HttpOperation *op, const char *code, const char *message) {
    clear_transport(op);
    op->result = component_error(code, message);
    wake_waiter(op);
}

static void poll_operation(HttpOperation *op) {
    if (op->exchange) {
        yyjson_mut_doc *metadata = op->metadata;
        yyjson_mut_val *root = yyjson_mut_doc_get_root(metadata);
        int status = 0;
        int child = component_http_poll(op->exchange, &status, metadata, op->headers);
        if (child > 0) {
            yyjson_val *request = yyjson_doc_get_root(op->input);
            yyjson_mut_obj_add_int(metadata, root, "body", child);
            yyjson_mut_obj_add_int(metadata, root, "status", status);
            yyjson_mut_obj_add_strcpy(metadata, root, "url", yyjson_get_str(yyjson_obj_get(request, "url")));
            yyjson_mut_obj_add_val(metadata, root, "headers", op->headers);
            op->result = component_json(metadata, 1);
            op->child = child;
        } else if (child == HTTP_FAILED) {
            op->result = component_error("ETRANSPORT", "WASI HTTP request failed");
        }
        if (child == HTTP_PENDING) { return; }
    } else {
        int count = component_http_read_poll(op->read);
        if (count == HTTP_PENDING) { return; }
        op->result = count < 0 ? component_error("ETRANSPORT", "WASI body read failed")
            : dream_status_bytes(1, component_http_read_bytes(op->read), (size_t)count);
    }
    clear_transport(op);
    wake_waiter(op);
}

void netBodyClose(int32_t body) {
    for (int i = 1; i < next_operation; i++) {
        HttpOperation *op = &operations[i];
        if (op->read && op->body == body) {
            abort_operation(op, "ECLOSED", "HTTP body closed during read");
        }
    }
    component_http_close(body);
}

void netOperationWait(dream_ptr future, int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live
        || operations[id].waiter || (!operations[id].exchange && !operations[id].read)) {
        dream_async_complete(future, 0);
        return;
    }
    HttpOperation *op = &operations[id];
    dream_retain(future);
    op->waiter = future;
    if (waiting_count++ == 0) { dream_component_register_http(poll_http); }
}

static int poll_http(void) {
    for (int i = 1; waiting_count && i < next_operation; i++) {
        HttpOperation *op = &operations[i];
        if (!op->waiter) { continue; }
        if (dream_async_host_status(op->waiter)) {
            abort_operation(op, "ECANCELLED", "HTTP operation cancelled");
        } else {
            poll_operation(op);
        }
    }
    return component_http_poll_cleanup() || waiting_count != 0;
}

dream_ptr netOperationPoll(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return component_error("ECLOSED", "HTTP operation is closed"); }
    dream_retain(operations[id].result); return operations[id].result;
}
void netOperationClaim(int32_t id) { if (id > 0 && id < COMPONENT_LIMIT) { operations[id].child = 0; } }
void netOperationRelease(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return; }
    HttpOperation *op = &operations[id];
    op->live = 0;
    clear_transport(op);
    wake_waiter(op);
    dream_release(op->result);
    op->result = 0;
    if (op->child) { netBodyClose(op->child); }
}
