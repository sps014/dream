#include "dream_rt_wasm32.h"

int32_t console_read_bytes(void *buffer, int32_t count) DREAM_WASM_IMPORT("env", "console_read_bytes");
int32_t console_write_bytes(int32_t stream, const void *buffer, int32_t count) DREAM_WASM_IMPORT("env", "console_write_bytes");
int32_t console_flush_stream(int32_t stream) DREAM_WASM_IMPORT("env", "console_flush_stream");

dream_ptr dream_console_read(int32_t count) {
    if (count < 0 || count == INT32_MAX) {
        return dream_status_bytes(1, NULL, 0);
    }
    dream_ptr bytes = dream_status_buffer(count);
    dream_status_finish(bytes, count ? console_read_bytes(dream_array_bytes(bytes) + 1, count) : 0);
    return bytes;
}

int32_t dream_console_write(int32_t stream, dream_ptr data) {
    int32_t count = data ? dream_i32(data)[0] : 0;
    return console_write_bytes(stream, data ? dream_array_bytes(data) : NULL, count);
}

int32_t dream_console_flush(int32_t stream) { return console_flush_stream(stream); }
