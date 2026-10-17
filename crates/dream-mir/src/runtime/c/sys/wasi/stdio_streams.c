#include "dream_rt_wasm32.h"

int32_t console_read_bytes(void *buffer, int32_t count) DREAM_WASM_IMPORT("env", "console_read_bytes");
int32_t console_write_bytes(int32_t stream, const void *buffer, int32_t count) DREAM_WASM_IMPORT("env", "console_write_bytes");
int32_t console_flush_stream(int32_t stream) DREAM_WASM_IMPORT("env", "console_flush_stream");

dream_ptr dream_console_read(int32_t count) {
    if (count < 0 || count == INT32_MAX) {
        dream_ptr error = dream_array_new(1, 1);
        ((uint8_t *)dream_p(error))[4] = 1;
        return error;
    }
    dream_ptr bytes = dream_array_new(count + 1, 1);
    uint8_t *payload = (uint8_t *)dream_p(bytes) + 4;
    int32_t n = count ? console_read_bytes(payload + 1, count) : 0;
    payload[0] = n < 0 ? 1 : 0;
    dream_i32(bytes)[0] = n < 0 ? 1 : n + 1;
    return bytes;
}

int32_t dream_console_write(int32_t stream, dream_ptr data) {
    int32_t count = data ? dream_i32(data)[0] : 0;
    return console_write_bytes(stream, data ? (uint8_t *)dream_p(data) + 4 : NULL, count);
}

int32_t dream_console_flush(int32_t stream) { return console_flush_stream(stream); }
