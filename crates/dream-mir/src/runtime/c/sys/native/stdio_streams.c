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
        return dream_status_bytes(1, NULL, 0);
    }
    dream_ptr bytes = dream_status_buffer(count);
    size_t n = count ? fread(dream_array_bytes(bytes) + 1, 1, (size_t)count, stdin) : 0;
    dream_status_finish(bytes, count && n == 0 && ferror(stdin) ? -1 : (int64_t)n);
    return bytes;
}

int32_t dream_console_write(int32_t stream, dream_ptr data) {
    FILE *output = stream == 1 ? stdout : stream == 2 ? stderr : NULL;
    if (!output) { return -1; }
    int32_t length = data ? dream_i32(data)[0] : 0;
    if (!length) { return 0; }
    if (!binary_stream(output)) { return -1; }
    size_t n = fwrite(dream_array_bytes(data), 1, (size_t)length, output);
    return n == 0 && ferror(output) ? -1 : (int32_t)n;
}

int32_t dream_console_flush(int32_t stream) {
    FILE *output = stream == 1 ? stdout : stream == 2 ? stderr : NULL;
    return output && fflush(output) == 0 ? 0 : -1;
}
