#ifndef DREAM_AUDIO_H
#define DREAM_AUDIO_H

#include <stddef.h>
#include <stdint.h>

/* Codecs use only platform memory and caller-owned input, so a background thread or a private
 * worker instance can run them. Word buffers come from the platform allocator.
 *
 * One-shot results are empty for unsupported input, a single negative status word (-1 limit,
 * -2 out of memory), or the sample rate, channel count and interleaved PCM. */
float *dream_audio_decode_words(const unsigned char *bytes, size_t length, int32_t max_bytes,
                                size_t *count);
float *dream_audio_decode_webm_words(const unsigned char *bytes, size_t size, int32_t max_bytes,
                                     size_t *count_out);
float *dream_audio_words_status(float status, size_t *count);

/* Status 0 requests more input, 1 carries PCM, 2 is the end; negative values are terminal:
 * -1 format, -2 memory, -3 channel count, -4 an element exceeded the compressed queue. */
typedef struct {
    int32_t status, rate, channels;
    const float *samples;
    size_t count;
} dream_audio_chunk;

/* Random access is available only for PCM and float WAV, whose frames decode independently. */
typedef struct {
    uint64_t data_position, block_align, frames;
} dream_audio_layout;

uintptr_t dream_audio_stream_open(int32_t capacity);
int32_t dream_audio_stream_space(uintptr_t handle);
int32_t dream_audio_stream_push(uintptr_t handle, const unsigned char *bytes, size_t length);
void dream_audio_stream_finish(uintptr_t handle);
dream_audio_chunk dream_audio_stream_decode(uintptr_t handle, int32_t max_frames);
double dream_audio_stream_duration(uintptr_t handle);
dream_audio_layout dream_audio_stream_layout(uintptr_t handle);
int64_t dream_audio_layout_offset(dream_audio_layout layout, int64_t frame);
int32_t dream_audio_stream_reposition(uintptr_t handle, int64_t frame);
void dream_audio_stream_close(uintptr_t handle);

#endif
