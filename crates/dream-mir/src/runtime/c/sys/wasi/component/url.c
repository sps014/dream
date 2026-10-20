#include "network.h"

static char *range(UriTextRangeA text) {
    size_t length = text.first && text.afterLast ? (size_t)(text.afterLast - text.first) : 0;
    char *result = malloc(length + 1);
    if (result) { memcpy(result, text.first ? text.first : "", length); result[length] = 0; }
    return result;
}

void component_url_free(ComponentUrl *url) {
    free(url->url); free(url->scheme); free(url->authority); free(url->path);
    memset(url, 0, sizeof(*url));
}

static char *serialize(const UriUriA *uri) {
    int length;
    if (uriToStringCharsRequiredA(uri, &length) || length < 0) { return NULL; }
    char *text = malloc((size_t)length + 1);
    if (text && uriToStringA(text, uri, length + 1, NULL)) { free(text); return NULL; }
    return text;
}

int component_url_parse(const char *input, const char *base, ComponentUrl *result) {
    UriUriA uri = {0}, parent = {0}, resolved = {0};
    int ok = 0;
    if (!input || uriParseSingleUriA(&uri, input, NULL)) { goto done; }
    UriUriA *value = &uri;
    if (base && *base) {
        if (uriParseSingleUriA(&parent, base, NULL) || uriAddBaseUriA(&resolved, &uri, &parent)) { goto done; }
        value = &resolved;
    }
    if (!value->scheme.first || value->userInfo.first || uriNormalizeSyntaxA(value)) { goto done; }
    result->url = serialize(value);
    result->scheme = range(value->scheme);
    if (!result->url || !result->scheme) { goto done; }
    const char *start = strstr(result->url, "://");
    if (!start) { goto done; }
    start += 3;
    const char *end = start + strcspn(start, "/?#");
    result->authority = malloc((size_t)(end - start) + 1);
    if (!result->authority) { goto done; }
    memcpy(result->authority, start, (size_t)(end - start));
    result->authority[end - start] = 0;
    const char *fragment = strchr(end, '#');
    size_t length = fragment ? (size_t)(fragment - end) : strlen(end);
    result->path = malloc(length + 2);
    if (!result->path) { goto done; }
    size_t prefix = *end != '/' ? 1 : 0;
    if (prefix) { result->path[0] = '/'; }
    memcpy(result->path + prefix, end, length);
    result->path[prefix + length] = 0;
    ok = 1;
done:
    uriFreeUriMembersA(&uri); uriFreeUriMembersA(&parent); uriFreeUriMembersA(&resolved);
    if (!ok) { component_url_free(result); }
    return ok;
}

dream_ptr netUrlParse(dream_ptr input, dream_ptr base) {
    char *text = dream_str_utf8(input), *parent = dream_str_utf8(base);
    ComponentUrl parsed = {0};
    if (!component_url_parse(text, parent, &parsed)) { free(text); free(parent); return dream_array_new(0, 4); }
    UriUriA uri = {0};
    if (uriParseSingleUriA(&uri, parsed.url, NULL)) { component_url_free(&parsed); free(text); free(parent); return dream_array_new(0, 4); }
    UriQueryListA *query = NULL;
    int count = 0;
    if (uri.query.first && uriDissectQueryMallocA(&query, &count, uri.query.first, uri.query.afterLast)) { count = 0; }
    dream_ptr result = dream_array_new(6 + count * 2, 4);
    dream_ptr *slots = (dream_ptr *)((uint8_t *)dream_p(result) + 4);
    char *root = strdup(parsed.url);
    char *tail = root ? strpbrk(root, "?#") : NULL; if (tail) { *tail = 0; }
    char *host = range(uri.hostText);
    char *path = strdup(parsed.path); tail = path ? strchr(path, '?') : NULL; if (tail) { *tail = 0; }
    slots[0] = dream_str_from_utf8(root ? root : ""); slots[1] = dream_str_from_utf8(parsed.scheme);
    slots[2] = dream_str_from_utf8(host ? host : ""); slots[3] = dream_str_from_utf8(path ? path : "");
    const char *fragment = strchr(parsed.url, '#'), *search = strchr(parsed.url, '?');
    slots[4] = dream_str_from_utf8(fragment ? fragment : "");
    char *q = search ? strdup(search) : strdup(""); tail = q ? strchr(q, '#') : NULL; if (tail) { *tail = 0; }
    slots[5] = dream_str_from_utf8(q ? q : "");
    int at = 6;
    for (UriQueryListA *item = query; item; item = item->next) {
        slots[at++] = dream_str_from_utf8(item->key);
        slots[at++] = dream_str_from_utf8(item->value);
    }
    free(root); free(host); free(path); free(q); free(text); free(parent);
    uriFreeQueryListA(query); uriFreeUriMembersA(&uri); component_url_free(&parsed);
    return result;
}
