#include "dream_core.h"
#include "dream_platform_internal.h"
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
#include <unistd.h>
#include <errno.h>

void platform_write(int stream, const void *bytes, size_t size, int encoding) {
    FILE *output = stream == 2 ? stderr : stdout;
    if (encoding == DREAM_TEXT_UTF16) {
        char buffer[1024];
        int32_t at = 0;
        while ((size_t)at < size) {
            size_t count;
            at = dream_utf8_chunk(bytes, (int32_t)size, buffer, sizeof(buffer), at, &count, 1);
            fwrite(buffer, 1, count, output);
        }
    } else { fwrite(bytes, 1, size, output); }
    fflush(output);
}

int32_t console_read_bytes(void *buffer, int32_t count) {
    if (count < 0) { return -1; }
    ssize_t result;
    do { result = read(0, buffer, (size_t)count); } while (result < 0 && errno == EINTR);
    return result < 0 ? -1 : (int32_t)result;
}

int32_t console_write_bytes(int32_t stream, const void *buffer, int32_t count) {
    if ((stream != 1 && stream != 2) || count < 0) { return -1; }
    ssize_t result;
    do { result = write(stream, buffer, (size_t)count); } while (result < 0 && errno == EINTR);
    return result < 0 ? -1 : (int32_t)result;
}

int32_t console_flush_stream(int32_t stream) {
    return stream == 1 ? fflush(stdout) : stream == 2 ? fflush(stderr) : -1;
}

static int64_t clock_nanos(clockid_t clock) {
    struct timespec value;
    if (clock_gettime(clock, &value)) { return 0; }
    return (int64_t)value.tv_sec * 1000000000 + value.tv_nsec;
}

int64_t timeNowNanos(void) { return clock_nanos(CLOCK_MONOTONIC); }
int64_t dateNowMillis(void) { return clock_nanos(CLOCK_REALTIME) / 1000000; }

void dream_component_wait_until(int64_t deadline) {
    int64_t remaining = deadline - timeNowNanos();
    if (remaining <= 0) { return; }
    struct timespec wait = {remaining / 1000000000, remaining % 1000000000};
    while (nanosleep(&wait, &wait) && errno == EINTR) {}
}

void async_host_cancel(dream_ptr future) { (void)future; }

dream_ptr cryptoSecureRandomBytes(int32_t count) {
    if (count < 0) { return 0; }
    dream_ptr result = dream_array_new(count, 1);
    uint8_t *bytes = (uint8_t *)dream_p(result) + LEN_PREFIX_SIZE;
    for (int32_t at = 0; at < count;) {
        size_t n = (size_t)(count - at);
        if (n > 256) { n = 256; }
        if (getentropy(bytes + at, n)) { dream_release(result); return 0; }
        at += (int32_t)n;
    }
    return result;
}
