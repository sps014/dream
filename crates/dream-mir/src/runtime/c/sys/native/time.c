#include "dream_core.h"
#include "dream_thread.h"

#include <time.h>

int64_t dateNowMillis(void) {
    return (int64_t)time(NULL) * 1000;
}

int64_t timeNowNanos(void) {
    return dream_monotonic_ns();
}

int64_t Time_nano_time(void) { return timeNowNanos(); }
