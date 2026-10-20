#include "dream_core.h"
#include "dream_platform_internal.h"
#include <math.h>
/* Validated sizes keep vendor diagnostics off the guest stderr path. */
#define NDEBUG
#define KISS_FFT_MALLOC(n) dream_platform_current->allocate(n)
#define KISS_FFT_FREE(p) dream_platform_current->deallocate(p)
#include "vendor/kissfft/kiss_fft.c"

/* The public analyzer validates sizes; checking again keeps the C boundary total. */
dream_ptr dream_audio_fft(dream_ptr source, int32_t size) {
    if (size < 32 || size > 32768 || (size & (size - 1)) ||
        !source || dream_i32(source)[0] != size) {
        return dream_array_new(0, 4);
    }
    kiss_fft_cfg cfg = kiss_fft_alloc(size, 0, NULL, NULL);
    kiss_fft_cpx *input = dream_raw_calloc((size_t)size, sizeof(*input));
    kiss_fft_cpx *output = dream_raw_calloc((size_t)size, sizeof(*output));
    if (!cfg || !input || !output) {
        KISS_FFT_FREE(cfg); KISS_FFT_FREE(input); KISS_FFT_FREE(output);
        return dream_array_new(0, 4);
    }
    const float *samples = (const float *)((const char *)dream_p(source) + 4);
    for (int32_t i = 0; i < size; ++i) input[i].r = samples[i];
    kiss_fft(cfg, input, output);
    dream_ptr result = dream_array_new(size + 2, 4);
    float *values = (float *)((char *)dream_p(result) + 4);
    for (int32_t i = 0; i <= size / 2; ++i) {
        values[2 * i] = output[i].r;
        values[2 * i + 1] = output[i].i;
    }
    KISS_FFT_FREE(cfg); KISS_FFT_FREE(input); KISS_FFT_FREE(output);
    return result;
}
