#include "dream_gpu_bundle.h"
#include "dream_gpu_bindings.h"
#include <stddef.h>

typedef struct {
    const uint8_t *bytes;
    uint32_t length, end, pos, count, capacity;
    uint32_t *records;
    int32_t error;
} reader;

enum { TRUNCATED = 1, MAGIC, VERSION, HEADER, UTF8, COUNT, TARGET, FORMAT,
       STAGE, BLOB, LAYOUT, KIND, FLAGS, BINDING, DUPLICATE, CAPACITY };

static uint32_t word(reader *r) {
    if (r->error) return 0;
    if (r->end - r->pos < 4) { r->error = TRUNCATED; return 0; }
    const uint8_t *p = r->bytes + r->pos;
    r->pos += 4;
    return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
}

static int equal(const reader *r, uint32_t offset, uint32_t n, const char *text) {
    uint32_t i = 0;
    while (text[i] && i < n && r->bytes[offset + i] == (uint8_t)text[i]) ++i;
    return i == n && !text[i];
}

static int utf8(const uint8_t *s, uint32_t length) {
    for (uint32_t i = 0; i < length;) {
        uint32_t c = s[i++], tail = 0, minimum = 0;
        if (c < 128) continue;
        if (c >= 194 && c <= 223) { c &= 31; tail = 1; minimum = 128; }
        else if (c >= 224 && c <= 239) { c &= 15; tail = 2; minimum = 2048; }
        else if (c >= 240 && c <= 244) { c &= 7; tail = 3; minimum = 65536; }
        else return 0;
        if (tail > length - i) return 0;
        while (tail--) {
            uint32_t next = s[i++];
            if ((next & 192) != 128) return 0;
            c = c << 6 | (next & 63);
        }
        if (c < minimum || c > 0x10ffff || (c >= 0xd800 && c <= 0xdfff)) return 0;
    }
    return 1;
}

static void text(reader *r, uint32_t *offset, uint32_t *length) {
    *length = word(r); *offset = r->pos;
    if (r->error) return;
    if (*length > r->end - r->pos) { r->error = TRUNCATED; return; }
    if (!utf8(r->bytes + r->pos, *length)) { r->error = UTF8; return; }
    r->pos += *length;
}

static uint32_t count(reader *r, uint32_t minimum) {
    uint32_t n = word(r);
    if (n > 65536 || n > (r->end - r->pos) / minimum) r->error = COUNT;
    return r->error ? 0 : n;
}

static void blob(reader *r, uint32_t start, uint32_t *out) {
    uint32_t offset = word(r), size = word(r), hash = r->pos;
    if (r->error) return;
    if (r->end - r->pos < 32) { r->error = TRUNCATED; return; }
    r->pos += 32;
    if (offset > r->length - start || size > r->length - start - offset) {
        r->error = BLOB; return;
    }
    out[0] = start + offset; out[1] = size; out[2] = hash;
}

static uint32_t record(reader *r, uint32_t *row) {
    uint32_t index = r->count++;
    if (index >= 65536) { r->error = COUNT; return index; }
    if (!r->records || r->error) return index;
    if (index >= r->capacity) { r->error = CAPACITY; return index; }
    if (row[0] == 1 || row[0] == 3) {
        for (uint32_t i = 0; i < index; ++i) {
            uint32_t *old = r->records + i * DREAM_GPU_BUNDLE_RECORD_WORDS;
            if (old[0] != row[0] || old[1] != row[1] || old[3] != row[3]) continue;
            uint32_t j = 0;
            while (j < row[3] && r->bytes[old[2] + j] == r->bytes[row[2] + j]) ++j;
            if (j == row[3]) { r->error = DUPLICATE; return index; }
        }
    }
    for (uint32_t i = 0; i < DREAM_GPU_BUNDLE_RECORD_WORDS; ++i)
        r->records[index * DREAM_GPU_BUNDLE_RECORD_WORDS + i] = row[i];
    return index;
}

static uint32_t backend(reader *r, uint32_t offset, uint32_t length) {
    static const char *const names[] = {"dxil", "dxbc", "spirv", "metal", "ptx", "wgsl"};
    for (uint32_t i = 0; i < 6; ++i) if (equal(r, offset, length, names[i])) return i + 1;
    r->error = TARGET; return 0;
}

static uint32_t stage(reader *r, uint32_t offset, uint32_t length) {
    static const char *const names[] = {"compute", "vertex", "fragment", "raygeneration",
        "closesthit", "anyhit", "miss", "intersection", "callable"};
    for (uint32_t i = 0; i < 9; ++i) if (equal(r, offset, length, names[i])) return i;
    r->error = STAGE; return 0;
}

static int valid_binding(uint32_t kind, uint32_t id) {
    return id ? dream_gpu_binding_kind(id) == kind : kind == 5 || kind == 7;
}

static void entry(reader *r, uint32_t parent, uint32_t start, uint32_t layouts, uint32_t webgpu) {
    uint32_t row[16] = {3, parent}, off = 0, len = 0;
    text(r, row + 2, row + 3); text(r, &off, &len);
    if (r->error) return;
    row[14] = off; row[15] = len;
    row[4] = stage(r, off, len); row[5] = word(r);
    if (row[5] >= (layouts ? layouts : 1)) r->error = LAYOUT;
    blob(r, start, row + 6);
    for (uint32_t i = 9; i < 12; ++i) row[i] = word(r);
    row[12] = count(r, 32);
    uint32_t index = record(r, row);
    for (uint32_t i = 0; i < row[12] && !r->error; ++i) {
        uint32_t param[16] = {4, index};
        text(r, param + 2, param + 3);
        for (uint32_t j = 4; j <= 10; ++j) param[j] = word(r);
        if (!param[4] || param[4] > 7) r->error = KIND;
        else if (param[5] & ~3u) r->error = FLAGS;
        else if (webgpu ? !valid_binding(param[4], param[10]) : param[10] != 0) r->error = BINDING;
        record(r, param);
    }
    uint32_t capabilities = count(r, 4);
    for (uint32_t i = 0; i < capabilities && !r->error; ++i) {
        uint32_t cap[16] = {5, index}; text(r, cap + 2, cap + 3); record(r, cap);
    }
}

uint32_t dream_gpu_bundle_abi_version(void) { return DREAM_GPU_BUNDLE_ABI_VERSION; }

int32_t dream_gpu_bundle_index(const uint8_t *bytes, uint32_t length, uint32_t *records, uint32_t capacity) {
    reader r = {bytes, length, length, 0, 0, capacity, records, 0};
    if (length < 16) return -TRUNCATED;
    if (!equal(&r, 0, 4, "DGPB")) return -MAGIC;
    r.pos = 4;
    if (word(&r)) return -VERSION;
    uint32_t start = word(&r);
    if (start < 16 || start > length) return -HEADER;
    r.end = start;
    uint32_t targets = count(&r, 16);
    for (uint32_t i = 0; i < targets && !r.error; ++i) {
        uint32_t row[16] = {1, UINT32_MAX};
        text(&r, row + 2, row + 3); text(&r, row + 4, row + 5);
        if (r.error) break;
        row[6] = backend(&r, row[2], row[3]);
        if (row[6] == 4) {
            if (!equal(&r, row[4], row[5], "msl") && !equal(&r, row[4], row[5], "metallib")) r.error = FORMAT;
        } else {
            uint32_t j = 0;
            while (j < row[3] && j < row[5] && bytes[row[2] + j] == bytes[row[4] + j]) ++j;
            if (j != row[3] || j != row[5]) r.error = FORMAT;
        }
        row[7] = count(&r, 40);
        uint32_t parent = record(&r, row);
        for (uint32_t j = 0; j < row[7] && !r.error; ++j) {
            uint32_t layout[16] = {2, parent}; blob(&r, start, layout + 2); record(&r, layout);
        }
        uint32_t entries = count(&r, 72);
        for (uint32_t j = 0; j < entries && !r.error; ++j) entry(&r, parent, start, row[7], row[6] == 6);
    }
    if (!r.error && r.pos != start) r.error = HEADER;
    return r.error ? -r.error : (int32_t)r.count;
}

const char *dream_gpu_bundle_error(int32_t status) {
    static const char *const errors[] = {"", "truncated header", "missing DGPB magic", "unsupported version",
        "header size does not match the target table", "string is not UTF-8", "count exceeds the header",
        "unknown target", "target cannot carry this code format", "unknown stage", "blob lies outside the data section",
        "entry names layout outside the target", "unknown parameter kind", "unknown parameter flags",
        "invalid WebGPU binding layout", "duplicate target or entry name", "record capacity is too small"};
    uint32_t code = status < 0 ? 0u - (uint32_t)status : 0;
    return code < sizeof(errors) / sizeof(errors[0]) ? errors[code] : "invalid bundle status";
}
