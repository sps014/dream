#ifdef DREAM_COMPONENT
#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_audio.h"
#include <stdlib.h>
#include <string.h>

/* Standalone components have no threads or JS workers, so each operation decodes inline and
 * its wait completes at once. */
typedef struct {
    float *words;
    size_t count;
} AudioDecodeJob;

uintptr_t audioDecodeStart(dream_ptr bytes, int32_t max_bytes) {
    AudioDecodeJob *job = calloc(1, sizeof(*job));
    if (!job) return 0;
    size_t length = bytes ? (size_t)dream_i32(bytes)[0] : 0;
    job->words = dream_audio_decode_words(length ? dream_array_bytes(bytes) : NULL, length,
                                          max_bytes, &job->count);
    return (uintptr_t)job;
}

void audioDecodeWait(dream_ptr future, uintptr_t handle) {
    (void)handle;
    dream_async_complete(future, 0);
}

dream_ptr audioDecodeResult(uintptr_t handle) {
    AudioDecodeJob *job = (AudioDecodeJob *)handle;
    size_t count = job ? job->count : 0;
    dream_ptr result = dream_array_new((int32_t)count, 4);
    if (count) memcpy(dream_array_bytes(result), job->words, count * sizeof(float));
    return result;
}

void audioDecodeRelease(uintptr_t handle) {
    AudioDecodeJob *job = (AudioDecodeJob *)handle;
    if (!job) return;
    dream_platform_current->deallocate(job->words);
    free(job);
}

enum { REQUEST_NONE, REQUEST_READ, REQUEST_REPOSITION };

typedef struct {
    uintptr_t decoder;
    int operation;
    int32_t value;
    dream_audio_chunk chunk;
} AudioDecodeStream;

uintptr_t audioDecodeStreamOpen(int32_t capacity) {
    uintptr_t decoder = dream_audio_stream_open(capacity);
    if (!decoder) return 0;
    AudioDecodeStream *s = calloc(1, sizeof(*s));
    if (!s) { dream_audio_stream_close(decoder); return 0; }
    s->decoder = decoder;
    return (uintptr_t)s;
}

int32_t audioDecodeStreamSpace(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    return s ? dream_audio_stream_space(s->decoder) : 0;
}

int32_t audioDecodeStreamPush(uintptr_t handle, dream_ptr bytes) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    size_t length = bytes ? (size_t)dream_i32(bytes)[0] : 0;
    if (!s) return 0;
    return dream_audio_stream_push(s->decoder, length ? dream_array_bytes(bytes) : NULL, length);
}

void audioDecodeStreamFinish(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (s) dream_audio_stream_finish(s->decoder);
}

int32_t audioDecodeStreamRead(uintptr_t handle, int32_t max_frames) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return 0;
    s->operation = REQUEST_READ;
    s->chunk = dream_audio_stream_decode(s->decoder, max_frames);
    return 1;
}

int32_t audioDecodeStreamReposition(uintptr_t handle, int64_t frame) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return 0;
    s->operation = REQUEST_REPOSITION;
    s->value = dream_audio_stream_reposition(s->decoder, frame);
    return 1;
}

void audioDecodeStreamWait(dream_ptr future, uintptr_t handle) {
    (void)handle;
    dream_async_complete(future, 0);
}

dream_ptr audioDecodeStreamResult(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s || s->operation == REQUEST_NONE) return dream_array_new(0, 4);
    dream_ptr result;
    if (s->operation == REQUEST_REPOSITION) {
        result = dream_array_new(1, 4);
        ((float *)dream_array_bytes(result))[0] = (float)s->value;
    } else {
        result = dream_array_new((int32_t)(s->chunk.count + 3), 4);
        float *words = (float *)dream_array_bytes(result);
        words[0] = (float)s->chunk.status;
        words[1] = (float)s->chunk.rate;
        words[2] = (float)s->chunk.channels;
        if (s->chunk.count) memcpy(words + 3, s->chunk.samples, s->chunk.count * sizeof(float));
    }
    s->operation = REQUEST_NONE;
    return result;
}

double audioDecodeStreamDuration(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    return s ? dream_audio_stream_duration(s->decoder) : -1.0;
}

int64_t audioDecodeStreamSeekOffset(uintptr_t handle, int64_t frame) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return -1;
    return dream_audio_layout_offset(dream_audio_stream_layout(s->decoder), frame);
}

void audioDecodeStreamClose(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return;
    dream_audio_stream_close(s->decoder);
    free(s);
}
#endif
