#include "network.h"

static struct { int live, child; dream_ptr result; } operations[COMPONENT_LIMIT];
static uint8_t clients[COMPONENT_LIMIT];
static int next_operation = 1, next_client = 1;

dream_ptr component_bytes(const void *bytes, size_t length) {
    if (length > INT32_MAX) { return 0; }
    dream_ptr result = dream_array_new((int32_t)length, 1);
    if (length) { memcpy((uint8_t *)dream_p(result) + 4, bytes, length); }
    return result;
}

dream_ptr component_json(yyjson_mut_doc *doc, int status) {
    size_t length;
    char *json = yyjson_mut_write(doc, 0, &length);
    if (!json || length >= INT32_MAX) { free(json); return 0; }
    dream_ptr result = dream_array_new((int32_t)length + 1, 1);
    uint8_t *bytes = (uint8_t *)dream_p(result) + 4;
    bytes[0] = (uint8_t)status;
    memcpy(bytes + 1, json, length);
    free(json);
    return result;
}

dream_ptr component_error(const char *code, const char *message) {
    yyjson_mut_doc *doc = yyjson_mut_doc_new(NULL);
    yyjson_mut_val *root = yyjson_mut_obj(doc);
    yyjson_mut_doc_set_root(doc, root);
    yyjson_mut_obj_add_str(doc, root, "code", code);
    yyjson_mut_obj_add_str(doc, root, "message", message);
    dream_ptr result = component_json(doc, 2);
    yyjson_mut_doc_free(doc);
    return result;
}

static int operation(dream_ptr result, int child) {
    if (next_operation == COMPONENT_LIMIT) { dream_release(result); if (child) { component_http_close(child); } return 0; }
    int id = next_operation++;
    operations[id].live = 1; operations[id].result = result; operations[id].child = child;
    return id;
}

int32_t netClientCreate(void) {
    if (next_client == COMPONENT_LIMIT) { return 0; }
    int id = next_client++; clients[id] = 1; return id;
}
void netClientClose(int32_t id) { if (id > 0 && id < COMPONENT_LIMIT) { clients[id] = 0; } }
int32_t netCapabilities(void) { return 13; }

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
    yyjson_mut_doc *metadata = yyjson_mut_doc_new(NULL);
    yyjson_mut_val *value = yyjson_mut_obj(metadata), *headers = yyjson_mut_arr(metadata);
    yyjson_mut_doc_set_root(metadata, value);
    int status = 0;
    int child = component_http_send(root, body, &status, metadata, headers);
    dream_ptr result;
    if (child > 0) {
        yyjson_mut_obj_add_int(metadata, value, "body", child);
        yyjson_mut_obj_add_int(metadata, value, "status", status);
        yyjson_mut_obj_add_strcpy(metadata, value, "url", yyjson_get_str(yyjson_obj_get(root, "url")));
        yyjson_mut_obj_add_val(metadata, value, "headers", headers);
        result = component_json(metadata, 1);
    } else { result = component_error("ETRANSPORT", "WASI HTTP request failed"); }
    yyjson_mut_doc_free(metadata); yyjson_doc_free(input); free(text);
    return operation(result, child);
}

int32_t netBodyRead(int32_t body, int32_t count) {
    if (count < 0 || count > 16 * 1024 * 1024) { return operation(component_error("EINVAL", "invalid body read size"), 0); }
    uint8_t *bytes = malloc((size_t)count + 1);
    if (!bytes) { return 0; }
    int length = component_http_read(body, bytes + 1, (size_t)count);
    dream_ptr result;
    if (length < 0) { result = component_error("ETRANSPORT", "WASI body read failed"); }
    else { bytes[0] = 1; result = component_bytes(bytes, (size_t)length + 1); }
    free(bytes);
    return operation(result, 0);
}
void netBodyClose(int32_t body) { component_http_close(body); }
void netOperationWait(dream_ptr future, int32_t id) { (void)id; dream_async_complete(future, 0); }
dream_ptr netOperationPoll(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return component_error("ECLOSED", "HTTP operation is closed"); }
    dream_retain(operations[id].result); return operations[id].result;
}
void netOperationClaim(int32_t id) { if (id > 0 && id < COMPONENT_LIMIT) { operations[id].child = 0; } }
void netOperationRelease(int32_t id) {
    if (id <= 0 || id >= COMPONENT_LIMIT || !operations[id].live) { return; }
    operations[id].live = 0;
    dream_release(operations[id].result);
    if (operations[id].child) { component_http_close(operations[id].child); }
}
