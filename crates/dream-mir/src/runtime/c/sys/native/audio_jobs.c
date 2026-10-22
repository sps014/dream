#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_audio.h"
#include "include/dream_blocking.h"
#include "include/dream_thread.h"
#include <stdlib.h>
#include <string.h>

/* Codecs run on the blocking pool so a long decode never stalls the scheduler. Only host-owned
 * copies cross into the pool, and Dream arrays are built back on the scheduler thread. The pool
 * holds its own reference, so an abandoned future never frees memory a decode still uses. */

typedef struct {
    dream_mutex mu;
    unsigned references;
    int done;
    int32_t max_bytes;
    unsigned char *bytes;
    size_t length;
    float *words;
    size_t count;
    dream_ptr waiter;
} AudioDecodeJob;

static void decode_job_drop(AudioDecodeJob *job) {
    dream_mutex_lock(&job->mu);
    unsigned references = --job->references;
    dream_mutex_unlock(&job->mu);
    if (references) return;
    free(job->bytes);
    dream_platform_current->deallocate(job->words);
    dream_mutex_destroy(&job->mu);
    free(job);
}

static void decode_job_run(void *context) {
    AudioDecodeJob *job = context;
    size_t count = 0;
    float *words = dream_audio_decode_words(job->bytes, job->length, job->max_bytes, &count);
    dream_mutex_lock(&job->mu);
    free(job->bytes);
    job->bytes = NULL;
    job->words = words;
    job->count = count;
    job->done = 1;
    dream_ptr waiter = job->waiter;
    job->waiter = 0;
    dream_mutex_unlock(&job->mu);
    if (waiter) dream_complete_foreign(waiter, 0);
    decode_job_drop(job);
}

uintptr_t audioDecodeStart(dream_ptr bytes, int32_t max_bytes) {
    size_t length = bytes ? (size_t)dream_i32(bytes)[0] : 0;
    AudioDecodeJob *job = calloc(1, sizeof(*job));
    if (!job) return 0;
    dream_mutex_init(&job->mu);
    job->references = 2;
    job->max_bytes = max_bytes;
    job->length = length;
    job->bytes = malloc(length ? length : 1);
    if (job->bytes && length) memcpy(job->bytes, dream_array_bytes(bytes), length);
    if (!job->bytes || !dream_blocking_submit(decode_job_run, job)) {
        job->references = 1;
        decode_job_drop(job);
        return 0;
    }
    return (uintptr_t)job;
}

int32_t audioDecodeWaitAsync(dream_ptr future, uintptr_t handle) {
    AudioDecodeJob *job = (AudioDecodeJob *)handle;
    if (!job) { dream_async_complete(future, 0); return 0; }
    dream_mutex_lock(&job->mu);
    if (job->done) {
        dream_mutex_unlock(&job->mu);
        dream_async_complete(future, 0);
        return 0;
    }
    job->waiter = future;
    dream_mutex_unlock(&job->mu);
    return 1;
}

dream_ptr audioDecodeResult(uintptr_t handle) {
    AudioDecodeJob *job = (AudioDecodeJob *)handle;
    if (!job) return dream_array_new(0, 4);
    dream_mutex_lock(&job->mu);
    size_t count = job->done ? job->count : 0;
    dream_ptr result = dream_array_new((int32_t)count, 4);
    if (count) memcpy(dream_array_bytes(result), job->words, count * sizeof(float));
    dream_mutex_unlock(&job->mu);
    return result;
}

void audioDecodeRelease(uintptr_t handle) {
    if (handle) decode_job_drop((AudioDecodeJob *)handle);
}

/* A stream decoder is touched only by its single in-flight pool job. The scheduler queues
 * encoded input beside it and reads snapshots of space, duration and seek layout that each job
 * refreshes, so no call from Dream waits for a decode. After a chunk is claimed the next one is
 * decoded ahead, keeping a ready chunk while the player drains the previous one. */
enum { REQUEST_NONE, REQUEST_READ, REQUEST_REPOSITION };

typedef struct {
    dream_mutex mu;
    unsigned references;
    uintptr_t decoder;
    int closed, running, busy, request, current, discard, finish_pending, finished;
    int32_t max_frames, space;
    int64_t frame;
    unsigned char *input;
    size_t input_length, input_allocated;
    double duration;
    dream_audio_layout layout;
    int operation, ready, ready_kind;
    int32_t status, rate, channels;
    float *samples;
    size_t count, allocated;
    dream_ptr waiter;
} AudioDecodeStream;

static void stream_drop(AudioDecodeStream *s) {
    dream_mutex_lock(&s->mu);
    unsigned references = --s->references;
    dream_mutex_unlock(&s->mu);
    if (references) return;
    dream_audio_stream_close(s->decoder);
    free(s->input);
    free(s->samples);
    dream_mutex_destroy(&s->mu);
    free(s);
}

/* Caller holds s->mu; returns the waiter to complete after unlocking. */
static dream_ptr stream_publish(AudioDecodeStream *s, int kind) {
    s->ready = 1;
    s->ready_kind = kind;
    if (s->operation != kind) return 0;
    dream_ptr waiter = s->waiter;
    s->waiter = 0;
    return waiter;
}

static void stream_run(void *context) {
    AudioDecodeStream *s = context;
    unsigned char *input = NULL;
    size_t input_allocated = 0;
    for (;;) {
        dream_mutex_lock(&s->mu);
        if (s->closed || s->request == REQUEST_NONE) {
            s->running = 0;
            dream_ptr waiter = s->waiter;
            s->waiter = 0;
            dream_mutex_unlock(&s->mu);
            if (waiter) dream_complete_foreign(waiter, 0);
            break;
        }
        int kind = s->current = s->request;
        int32_t max_frames = s->max_frames;
        int64_t frame = s->frame;
        int finish = s->finish_pending;
        s->request = REQUEST_NONE;
        s->busy = 1;
        s->discard = 0;
        s->finish_pending = 0;
        unsigned char *queued = s->input;
        size_t queued_length = s->input_length, queued_allocated = s->input_allocated;
        s->input = input;
        s->input_allocated = input_allocated;
        s->input_length = 0;
        input = queued;
        input_allocated = queued_allocated;
        dream_mutex_unlock(&s->mu);

        int32_t status = 0, value = 0;
        dream_audio_chunk chunk = {0};
        if (queued_length && !dream_audio_stream_push(s->decoder, input, queued_length)) {
            status = -2;
        } else if (kind == REQUEST_REPOSITION) {
            value = dream_audio_stream_reposition(s->decoder, frame);
        } else {
            if (finish) dream_audio_stream_finish(s->decoder);
            chunk = dream_audio_stream_decode(s->decoder, max_frames);
            status = chunk.status;
        }

        dream_mutex_lock(&s->mu);
        if (kind == REQUEST_READ && status == 0 && !s->discard && s->request == REQUEST_NONE &&
            (s->input_length || s->finish_pending)) {
            s->request = REQUEST_READ;
            dream_mutex_unlock(&s->mu);
            continue;
        }
        s->busy = 0;
        s->space = dream_audio_stream_space(s->decoder);
        s->duration = dream_audio_stream_duration(s->decoder);
        s->layout = dream_audio_stream_layout(s->decoder);
        dream_ptr waiter = 0;
        if (!s->discard) {
            if (kind == REQUEST_REPOSITION) {
                s->status = status < 0 ? 0 : value;
                s->count = 0;
            } else {
                s->status = status;
                s->rate = chunk.rate;
                s->channels = chunk.channels;
                s->count = 0;
                if (chunk.count > s->allocated) {
                    float *grown = realloc(s->samples, chunk.count * sizeof(float));
                    if (grown) {
                        s->samples = grown;
                        s->allocated = chunk.count;
                    } else {
                        s->status = -2;
                    }
                }
                if (s->status > 0 && chunk.count) {
                    memcpy(s->samples, chunk.samples, chunk.count * sizeof(float));
                    s->count = chunk.count;
                }
            }
            waiter = stream_publish(s, kind);
        }
        dream_mutex_unlock(&s->mu);
        if (waiter) dream_complete_foreign(waiter, 0);
    }
    free(input);
    stream_drop(s);
}

/* Caller holds s->mu. A rejected submission leaves the request for the next operation. */
static int stream_schedule(AudioDecodeStream *s) {
    if (s->running) return 1;
    s->running = 1;
    s->references += 1;
    if (dream_blocking_submit(stream_run, s)) return 1;
    s->running = 0;
    s->references -= 1;
    s->request = REQUEST_NONE;
    return 0;
}

uintptr_t audioDecodeStreamOpen(int32_t capacity) {
    uintptr_t decoder = dream_audio_stream_open(capacity);
    if (!decoder) return 0;
    AudioDecodeStream *s = calloc(1, sizeof(*s));
    if (!s) { dream_audio_stream_close(decoder); return 0; }
    dream_mutex_init(&s->mu);
    s->references = 1;
    s->decoder = decoder;
    s->space = dream_audio_stream_space(decoder);
    s->duration = -1.0;
    return (uintptr_t)s;
}

int32_t audioDecodeStreamSpace(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return 0;
    dream_mutex_lock(&s->mu);
    int64_t space = s->finished ? 0 : (int64_t)s->space - (int64_t)s->input_length;
    dream_mutex_unlock(&s->mu);
    return space > 0 ? (int32_t)space : 0;
}

int32_t audioDecodeStreamPush(uintptr_t handle, dream_ptr bytes) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    size_t length = bytes ? (size_t)dream_i32(bytes)[0] : 0;
    if (!s) return 0;
    dream_mutex_lock(&s->mu);
    int accepted = !s->finished && (int64_t)length <= (int64_t)s->space - (int64_t)s->input_length;
    if (accepted && s->input_length + length > s->input_allocated) {
        size_t next = s->input_allocated ? s->input_allocated * 2 : 65536;
        if (next < s->input_length + length) next = s->input_length + length;
        unsigned char *grown = realloc(s->input, next);
        if (grown) {
            s->input = grown;
            s->input_allocated = next;
        } else {
            accepted = 0;
        }
    }
    if (accepted && length) {
        memcpy(s->input + s->input_length, dream_array_bytes(bytes), length);
        s->input_length += length;
    }
    dream_mutex_unlock(&s->mu);
    return accepted;
}

void audioDecodeStreamFinish(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return;
    dream_mutex_lock(&s->mu);
    s->finished = 1;
    s->finish_pending = 1;
    dream_mutex_unlock(&s->mu);
}

int32_t audioDecodeStreamRead(uintptr_t handle, int32_t max_frames) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return 0;
    dream_mutex_lock(&s->mu);
    s->operation = REQUEST_READ;
    int started = 1;
    int pending = s->request == REQUEST_READ ||
                  (s->busy && s->current == REQUEST_READ && !s->discard);
    if (!(s->ready && s->ready_kind == REQUEST_READ) && !pending) {
        s->ready = 0;
        s->request = REQUEST_READ;
        s->max_frames = max_frames;
        started = stream_schedule(s);
    }
    dream_mutex_unlock(&s->mu);
    return started;
}

int32_t audioDecodeStreamReposition(uintptr_t handle, int64_t frame) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return 0;
    dream_mutex_lock(&s->mu);
    s->operation = REQUEST_REPOSITION;
    s->ready = 0;
    s->discard = s->running;
    s->input_length = 0;
    s->finished = 0;
    s->finish_pending = 0;
    s->request = REQUEST_REPOSITION;
    s->frame = frame;
    int started = stream_schedule(s);
    dream_mutex_unlock(&s->mu);
    return started;
}

int32_t audioDecodeStreamWaitAsync(dream_ptr future, uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) { dream_async_complete(future, 0); return 0; }
    dream_mutex_lock(&s->mu);
    if ((s->ready && s->ready_kind == s->operation) || !s->running) {
        dream_mutex_unlock(&s->mu);
        dream_async_complete(future, 0);
        return 0;
    }
    /* A cancelled earlier wait still holds a scheduler credit until it is completed. */
    dream_ptr previous = s->waiter;
    s->waiter = future;
    dream_mutex_unlock(&s->mu);
    if (previous) dream_complete_foreign(previous, 0);
    return 1;
}

/* Reads yield status, sample rate, channels and PCM; repositioning yields one success word. */
dream_ptr audioDecodeStreamResult(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return dream_array_new(0, 4);
    dream_mutex_lock(&s->mu);
    dream_ptr result;
    if (!s->ready || s->ready_kind != s->operation) {
        result = dream_array_new(s->operation == REQUEST_READ ? 3 : 1, 4);
        ((float *)dream_array_bytes(result))[0] = s->operation == REQUEST_READ ? -2.0f : 0.0f;
    } else if (s->ready_kind == REQUEST_REPOSITION) {
        result = dream_array_new(1, 4);
        ((float *)dream_array_bytes(result))[0] = (float)s->status;
    } else {
        result = dream_array_new((int32_t)(s->count + 3), 4);
        float *words = (float *)dream_array_bytes(result);
        words[0] = (float)s->status;
        words[1] = (float)s->rate;
        words[2] = (float)s->channels;
        if (s->count) memcpy(words + 3, s->samples, s->count * sizeof(float));
    }
    int prefetch = s->ready && s->ready_kind == REQUEST_READ && s->status == 1 && !s->closed;
    s->ready = 0;
    s->operation = REQUEST_NONE;
    if (prefetch) {
        s->request = REQUEST_READ;
        stream_schedule(s);
    }
    dream_mutex_unlock(&s->mu);
    return result;
}

double audioDecodeStreamDuration(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return -1.0;
    dream_mutex_lock(&s->mu);
    double duration = s->duration;
    dream_mutex_unlock(&s->mu);
    return duration;
}

int64_t audioDecodeStreamSeekOffset(uintptr_t handle, int64_t frame) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return -1;
    dream_mutex_lock(&s->mu);
    dream_audio_layout layout = s->layout;
    dream_mutex_unlock(&s->mu);
    return dream_audio_layout_offset(layout, frame);
}

void audioDecodeStreamClose(uintptr_t handle) {
    AudioDecodeStream *s = (AudioDecodeStream *)handle;
    if (!s) return;
    dream_mutex_lock(&s->mu);
    s->closed = 1;
    dream_mutex_unlock(&s->mu);
    stream_drop(s);
}
