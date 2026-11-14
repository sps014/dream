#include "audio_host.h"

#include <math.h>

/* Validated sizes keep vendor diagnostics off the guest stderr path. */
#define NDEBUG
#define KISS_FFT_MALLOC(n) dream_platform_current->allocate(n)
#define KISS_FFT_FREE(p) dream_platform_current->deallocate(p)
#include "kissfft/kiss_fft.c"

/* `count` is the caller array length. A rejected size returns an empty array. */
void *dream_audio_fft(const float *samples, int32_t count, int32_t size) {
    if (size < 32 || size > 32768 || (size & (size - 1)) || !samples || count != size) {
        return dream_scalar_array_new(4, 0);
    }
    kiss_fft_cfg cfg = kiss_fft_alloc(size, 0, NULL, NULL);
    kiss_fft_cpx *input = dream_raw_calloc((size_t)size, sizeof(*input));
    kiss_fft_cpx *output = dream_raw_calloc((size_t)size, sizeof(*output));
    if (!cfg || !input || !output) {
        KISS_FFT_FREE(cfg);
        KISS_FFT_FREE(input);
        KISS_FFT_FREE(output);
        return dream_scalar_array_new(4, 0);
    }
    for (int32_t i = 0; i < size; ++i) {
        input[i].r = samples[i];
    }
    kiss_fft(cfg, input, output);
    void *result = dream_scalar_array_new(4, size + 2);
    float *values = result ? (float *)dream_array_data(result) : NULL;
    if (values) {
        for (int32_t i = 0; i <= size / 2; ++i) {
            values[2 * i] = output[i].r;
            values[2 * i + 1] = output[i].i;
        }
    }
    KISS_FFT_FREE(cfg);
    KISS_FFT_FREE(input);
    KISS_FFT_FREE(output);
    return result ? result : dream_scalar_array_new(4, 0);
}
