#ifndef DREAM_COMPONENT
#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_audio.h"

/* Entry points for the JS decode worker. It instantiates this module again with private memory
 * and never runs the program, so these touch only the codecs and the platform heap. Results are
 * reported through one static record that the worker reads before its next call. */
typedef struct {
    int32_t status, rate, channels, space;
    uint32_t samples, count;
    double duration;
    uint64_t data_position, block_align, frames;
} AudioWorkerReport;

_Static_assert(sizeof(AudioWorkerReport) == 56, "the JS decode worker reads fixed offsets");

static AudioWorkerReport report;

void *dream_audio_worker_alloc(uint32_t size) {
    return dream_platform_current->allocate(size ? size : 1);
}

void dream_audio_worker_free(void *pointer) {
    dream_platform_current->deallocate(pointer);
}

AudioWorkerReport *dream_audio_worker_decode(const unsigned char *bytes, uint32_t length,
                                             int32_t max_bytes) {
    size_t count = 0;
    float *words = dream_audio_decode_words(bytes, length, max_bytes, &count);
    report = (AudioWorkerReport){0};
    report.samples = (uint32_t)(uintptr_t)words;
    report.count = (uint32_t)count;
    return &report;
}

uintptr_t dream_audio_worker_stream_open(int32_t capacity) {
    return dream_audio_stream_open(capacity);
}

static void report_stream(uintptr_t decoder) {
    dream_audio_layout layout = dream_audio_stream_layout(decoder);
    report.space = dream_audio_stream_space(decoder);
    report.duration = dream_audio_stream_duration(decoder);
    report.data_position = layout.data_position;
    report.block_align = layout.block_align;
    report.frames = layout.frames;
}

/* Input pushed here was admitted against the space the previous report announced. */
AudioWorkerReport *dream_audio_worker_stream_read(uintptr_t decoder, const unsigned char *bytes,
                                                  uint32_t length, int32_t finish,
                                                  int32_t max_frames) {
    report = (AudioWorkerReport){0};
    if (length && !dream_audio_stream_push(decoder, bytes, length)) {
        report.status = -2;
    } else {
        if (finish) dream_audio_stream_finish(decoder);
        dream_audio_chunk chunk = dream_audio_stream_decode(decoder, max_frames);
        report.status = chunk.status;
        report.rate = chunk.rate;
        report.channels = chunk.channels;
        report.samples = (uint32_t)(uintptr_t)chunk.samples;
        report.count = (uint32_t)chunk.count;
    }
    report_stream(decoder);
    return &report;
}

AudioWorkerReport *dream_audio_worker_stream_reposition(uintptr_t decoder, int64_t frame) {
    report = (AudioWorkerReport){0};
    report.status = dream_audio_stream_reposition(decoder, frame);
    report_stream(decoder);
    return &report;
}

void dream_audio_worker_stream_close(uintptr_t decoder) {
    dream_audio_stream_close(decoder);
}
#endif
