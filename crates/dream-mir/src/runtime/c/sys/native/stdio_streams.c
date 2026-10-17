#include "dream_host_support.h"

#include <stdio.h>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

static int binary_stream(FILE *stream) {
#ifdef _WIN32
    int fd = _fileno(stream);
    return fd >= 0 && _setmode(fd, _O_BINARY) >= 0;
#else
    (void)stream;
    return 1;
#endif
}

dream_ptr dream_console_read(int32_t count) {
    if (count < 0 || count == INT32_MAX || (count > 0 && !binary_stream(stdin))) {
        dream_ptr error = dream_array_new(1, 1);
        ((uint8_t *)dream_p(error))[4] = 1;
        return error;
    }
    dream_ptr bytes = dream_array_new(count + 1, 1);
    uint8_t *payload = (uint8_t *)dream_p(bytes) + 4;
    size_t n = count ? fread(payload + 1, 1, (size_t)count, stdin) : 0;
    payload[0] = count && n == 0 && ferror(stdin) ? 1 : 0;
    dream_i32(bytes)[0] = (int32_t)n + 1;
    return bytes;
}

int32_t dream_console_write(int32_t stream, dream_ptr data) {
    FILE *output = stream == 1 ? stdout : stream == 2 ? stderr : NULL;
    if (!output) { return -1; }
    int32_t length = data ? dream_i32(data)[0] : 0;
    if (!length) { return 0; }
    if (!binary_stream(output)) { return -1; }
    size_t n = fwrite((uint8_t *)dream_p(data) + 4, 1, (size_t)length, output);
    return n == 0 && ferror(output) ? -1 : (int32_t)n;
}

int32_t dream_console_flush(int32_t stream) {
    FILE *output = stream == 1 ? stdout : stream == 2 ? stderr : NULL;
    return output && fflush(output) == 0 ? 0 : -1;
}
