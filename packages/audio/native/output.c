#define MINIAUDIO_IMPLEMENTATION
#define MA_NO_DECODING
#define MA_NO_ENCODING
#define MA_NO_RESOURCE_MANAGER
#define MA_NO_GENERATION
#include "miniaudio.h"

/* Host entry points serialize access. Miniaudio owns the realtime mixing threads. */
static ma_context context;
static int initialized;
typedef struct { ma_engine engine; ma_device_id id; int is_default; int refs; } output;
static output outputs[16];
#define CAPTURE_FRAMES 32768
typedef struct {
    ma_atomic_uint64 written;
    ma_atomic_uint64 writer;
    ma_atomic_uint32 sequence;
    ma_atomic_float samples[CAPTURE_FRAMES * 2];
    ma_uint64 last_read;
} capture_data;
typedef struct { ma_node_base base; capture_data *data; } capture_node;

/*
 * Single-producer ring fed by guest writes and drained by the audio callback. Counters
 * are absolute frame indexes. A flush publishes `skip`; the reader jumps past older
 * frames, so the writer never touches the reader's index. Underruns emit silence
 * without advancing the read index.
 */
typedef struct {
    ma_data_source_base base;
    ma_atomic_float *samples;
    ma_uint32 channels, rate;
    ma_uint64 capacity;
    ma_atomic_uint64 read, write, skip;
    ma_atomic_uint32 eos, starved, ended;
    ma_uint64 position_base;
} stream_source;

typedef struct {
    ma_audio_buffer buffer;
    stream_source *stream;
    ma_data_source *source;
    ma_sound *sound;
    output *out;
    capture_node *tap;
} player;

static ma_uint64 stream_read_index(stream_source *s) {
    ma_uint64 read = ma_atomic_uint64_get(&s->read), skip = ma_atomic_uint64_get(&s->skip);
    return read > skip ? read : skip;
}
static ma_result stream_on_read(ma_data_source *source, void *frames_out, ma_uint64 frame_count, ma_uint64 *frames_read) {
    stream_source *s = source;
    float *out = frames_out;
    ma_uint64 read = stream_read_index(s), write = ma_atomic_uint64_get(&s->write);
    ma_uint64 count = write - read < frame_count ? write - read : frame_count;
    for (ma_uint64 frame = 0; frame < count; ++frame) {
        ma_uint64 slot = ((read + frame) % s->capacity) * s->channels;
        for (ma_uint32 channel = 0; channel < s->channels; ++channel) {
            out[frame * s->channels + channel] = ma_atomic_float_get(&s->samples[slot + channel]);
        }
    }
    ma_atomic_uint64_set(&s->read, read + count);
    int eos = ma_atomic_uint32_get(&s->eos) != 0;
    if (count == frame_count) {
        ma_atomic_uint32_set(&s->starved, 0);
        *frames_read = count;
        return MA_SUCCESS;
    }
    if (eos) {
        *frames_read = count;
        if (count) return MA_SUCCESS;
        ma_atomic_uint32_set(&s->ended, 1);
        return MA_AT_END;
    }
    memset(out + count * s->channels, 0, (size_t)(frame_count - count) * s->channels * sizeof(float));
    ma_atomic_uint32_set(&s->starved, 1);
    *frames_read = frame_count;
    return MA_SUCCESS;
}
static ma_result stream_on_seek(ma_data_source *source, ma_uint64 frame) {
    (void)source; (void)frame;
    return MA_SUCCESS;
}
static ma_result stream_on_format(ma_data_source *source, ma_format *format, ma_uint32 *channels,
                                  ma_uint32 *rate, ma_channel *map, size_t map_capacity) {
    stream_source *s = source;
    *format = ma_format_f32;
    *channels = s->channels;
    *rate = s->rate;
    ma_channel_map_init_standard(ma_standard_channel_map_default, map, map_capacity, s->channels);
    return MA_SUCCESS;
}
static ma_result stream_on_unknown(ma_data_source *source, ma_uint64 *value) {
    (void)source; *value = 0;
    return MA_NOT_IMPLEMENTED;
}
static ma_data_source_vtable stream_vtable = {
    stream_on_read, stream_on_seek, stream_on_format, stream_on_unknown, stream_on_unknown, NULL, 0
};

static void capture_process(ma_node *node, const float **input, ma_uint32 *input_count,
                            float **output_frames, ma_uint32 *output_count) {
    capture_data *data = ((capture_node *)node)->data;
    ma_uint32 count = *output_count;
    if (count > *input_count) count = *input_count;
    memcpy(output_frames[0], input[0], count * 2 * sizeof(float));
    *input_count = count;
    *output_count = count;
    /* Sink preparation can temporarily attach two taps; only the committed node may publish. */
    if (ma_atomic_uint64_get(&data->writer) != (ma_uint64)(ma_uintptr)node) return;
    ma_uint64 written = ma_atomic_uint64_get(&data->written);
    ma_uint32 sequence = ma_atomic_uint32_get(&data->sequence);
    ma_atomic_uint32_set(&data->sequence, sequence + 1);
    for (ma_uint32 frame = 0; frame < count; ++frame) {
        ma_uint32 slot = (ma_uint32)((written + frame) % CAPTURE_FRAMES) * 2;
        ma_atomic_float_set(&data->samples[slot], input[0][frame * 2]);
        ma_atomic_float_set(&data->samples[slot + 1], input[0][frame * 2 + 1]);
    }
    ma_atomic_uint64_set(&data->written, written + count);
    ma_atomic_uint32_set(&data->sequence, sequence + 2);
    *input_count = count;
    *output_count = count;
}
static ma_node_vtable capture_vtable = { capture_process, NULL, 1, 1, 0 };

static capture_node *attach_capture(output *o, ma_sound *sound, capture_data *data) {
    capture_node *tap = calloc(1, sizeof(*tap));
    if (!tap) return NULL;
    tap->data = data;
    ma_uint32 channels = 2;
    ma_node_config config = ma_node_config_init();
    config.vtable = &capture_vtable;
    config.pInputChannels = &channels;
    config.pOutputChannels = &channels;
    if (ma_node_init(ma_engine_get_node_graph(&o->engine), &config, NULL, &tap->base) != MA_SUCCESS) {
        free(tap); return NULL;
    }
    if (ma_node_attach_output_bus(tap, 0, ma_node_graph_get_endpoint(ma_engine_get_node_graph(&o->engine)), 0) != MA_SUCCESS ||
        ma_node_attach_output_bus(sound, 0, tap, 0) != MA_SUCCESS) {
        ma_node_uninit(tap, NULL); free(tap); return NULL;
    }
    return tap;
}

static int ensure_context(void) {
    if (!initialized) {
        if (ma_context_init(NULL, 0, NULL, &context) != MA_SUCCESS) return 0;
        initialized = 1;
    }
    return 1;
}
void dream_audio_context_idle(void) {
    for (int i = 0; i < 16; ++i) if (outputs[i].refs) return;
    if (initialized) { ma_context_uninit(&context); initialized = 0; }
}
int dream_audio_device_count(void) {
    ma_device_info *devices; ma_uint32 count;
    if (!ensure_context() || ma_context_get_devices(&context, &devices, &count, NULL, NULL) != MA_SUCCESS) return 0;
    return (int)count;
}
const char *dream_audio_device_name(int index) {
    ma_device_info *devices; ma_uint32 count;
    if (!ensure_context() || ma_context_get_devices(&context, &devices, &count, NULL, NULL) != MA_SUCCESS || index < 0 || (ma_uint32)index >= count) return NULL;
    return devices[index].name;
}
int dream_audio_device_default(int index) {
    ma_device_info *devices; ma_uint32 count;
    if (!ensure_context() || ma_context_get_devices(&context, &devices, &count, NULL, NULL) != MA_SUCCESS || index < 0 || (ma_uint32)index >= count) return 0;
    return devices[index].isDefault;
}
static output *acquire_output(int index) {
    if (!ensure_context()) return NULL;
    ma_device_info *devices = NULL; ma_uint32 count = 0;
    if (index >= 0 && (ma_context_get_devices(&context, &devices, &count, NULL, NULL) != MA_SUCCESS || (ma_uint32)index >= count)) return NULL;
    ma_device_id selected;
    if (index >= 0) selected = devices[index].id;
    for (int i = 0; i < 16; ++i) {
        output *o = &outputs[i];
        if (o->refs && ((index < 0 && o->is_default) || (index >= 0 && !o->is_default && memcmp(&o->id, &selected, sizeof(selected)) == 0))) {
            ++o->refs; return o;
        }
    }
    for (int i = 0; i < 16; ++i) {
        output *o = &outputs[i];
        if (o->refs) continue;
        ma_engine_config config = ma_engine_config_init();
        config.pContext = &context; config.channels = 2; config.sampleRate = 48000;
        if (index >= 0) config.pPlaybackDeviceID = &selected;
        if (ma_engine_init(&config, &o->engine) != MA_SUCCESS) return NULL;
        o->is_default = index < 0;
        if (index >= 0) o->id = selected;
        o->refs = 1; return o;
    }
    return NULL;
}
static void release_output(output *o) {
    if (o && --o->refs == 0) ma_engine_uninit(&o->engine);
    dream_audio_context_idle();
}
void *dream_audio_output_create(const float *samples, int count, int rate, int channels) {
    if (!samples || count <= 0 || rate <= 0 || channels < 1 || channels > 2 || count % channels) return NULL;
    output *o = acquire_output(-1);
    if (!o) return NULL;
    player *p = calloc(1, sizeof(*p));
    if (!p) { release_output(o); return NULL; }
    ma_audio_buffer_config config = ma_audio_buffer_config_init(ma_format_f32, (ma_uint32)channels, (ma_uint64)(count / channels), samples, NULL);
    if (ma_audio_buffer_init_copy(&config, &p->buffer) != MA_SUCCESS) { free(p); release_output(o); return NULL; }
    p->buffer.ref.sampleRate = (ma_uint32)rate;
    p->sound = calloc(1, sizeof(*p->sound));
    if (!p->sound) { ma_audio_buffer_uninit(&p->buffer); free(p); release_output(o); return NULL; }
    ma_data_source_set_range_in_pcm_frames(&p->buffer, 0, (ma_uint64)(count / channels));
    if (ma_sound_init_from_data_source(&o->engine, &p->buffer, MA_SOUND_FLAG_NO_SPATIALIZATION, NULL, p->sound) != MA_SUCCESS) {
        ma_audio_buffer_uninit(&p->buffer); free(p->sound); free(p); release_output(o); return NULL;
    }
    p->source = &p->buffer;
    p->out = o; return p;
}
void *dream_audio_output_stream(int rate, int channels, int frames) {
    if (rate <= 0 || rate > 384000 || channels < 1 || channels > 2 || frames <= 0 || frames > rate * 30) return NULL;
    output *o = acquire_output(-1);
    if (!o) return NULL;
    player *p = calloc(1, sizeof(*p));
    stream_source *s = calloc(1, sizeof(*s));
    ma_atomic_float *samples = calloc((size_t)frames * (size_t)channels, sizeof(*samples));
    ma_sound *sound = calloc(1, sizeof(*sound));
    ma_data_source_config config = ma_data_source_config_init();
    config.vtable = &stream_vtable;
    if (!p || !s || !samples || !sound || ma_data_source_init(&config, &s->base) != MA_SUCCESS) {
        free(p); free(s); free(samples); free(sound); release_output(o); return NULL;
    }
    s->samples = samples;
    s->channels = (ma_uint32)channels;
    s->rate = (ma_uint32)rate;
    s->capacity = (ma_uint64)frames;
    if (ma_sound_init_from_data_source(&o->engine, s, MA_SOUND_FLAG_NO_SPATIALIZATION, NULL, sound) != MA_SUCCESS) {
        ma_data_source_uninit(s); free(p); free(s); free(samples); free(sound); release_output(o); return NULL;
    }
    p->stream = s;
    p->source = s;
    p->sound = sound;
    p->out = o;
    return p;
}
/* Accepts whole frames up to the free ring space; returns the number of frames queued. */
int dream_audio_output_write(void *handle, const float *samples, int count) {
    player *p = handle;
    if (!p || !p->stream || !samples || count < 0) return 0;
    stream_source *s = p->stream;
    ma_uint64 write = ma_atomic_uint64_get(&s->write);
    ma_uint64 space = s->capacity - (write - stream_read_index(s));
    ma_uint64 frames = (ma_uint64)count / s->channels;
    if (frames > space) frames = space;
    for (ma_uint64 frame = 0; frame < frames; ++frame) {
        ma_uint64 slot = ((write + frame) % s->capacity) * s->channels;
        for (ma_uint32 channel = 0; channel < s->channels; ++channel) {
            ma_atomic_float_set(&s->samples[slot + channel], samples[frame * s->channels + channel]);
        }
    }
    ma_atomic_uint64_set(&s->write, write + frames);
    return (int)frames;
}
void dream_audio_output_close(void *handle) {
    player *p = handle; if (!p) return;
    ma_sound_uninit(p->sound); free(p->sound);
    if (p->stream) {
        ma_data_source_uninit(p->stream);
        free(p->stream->samples);
        free(p->stream);
    } else {
        ma_audio_buffer_uninit(&p->buffer);
    }
    if (p->tap) { ma_node_uninit(p->tap, NULL); free(p->tap->data); free(p->tap); }
    release_output(p->out); free(p);
}

/* Atomic slots plus a sequence check bound the reader without blocking the audio thread. */
int dream_audio_output_capture(void *handle, int frames, float *result) {
    player *p = handle;
    if (!p || frames < 0 || frames > CAPTURE_FRAMES || !result) return 0;
    if (!p->tap) {
        capture_data *data = calloc(1, sizeof(*data));
        if (!data) return 0;
        p->tap = attach_capture(p->out, p->sound, data);
        if (!p->tap) { free(data); return 0; }
        ma_atomic_uint64_set(&data->writer, (ma_uint64)(ma_uintptr)p->tap);
    }
    capture_data *data = p->tap->data;
    result[0] = (float)ma_engine_get_sample_rate(&p->out->engine);
    result[1] = 2;
    for (int attempt = 0; attempt < 3; ++attempt) {
        ma_uint32 sequence = ma_atomic_uint32_get(&data->sequence);
        if (sequence & 1) continue;
        ma_uint64 end = ma_atomic_uint64_get(&data->written);
        ma_uint32 count = end < (ma_uint64)frames ? (ma_uint32)end : (ma_uint32)frames;
        ma_uint64 start = end - count;
        for (ma_uint32 frame = 0; frame < count; ++frame) {
            ma_uint32 slot = (ma_uint32)((start + frame) % CAPTURE_FRAMES) * 2;
            result[5 + frame * 2] = ma_atomic_float_get(&data->samples[slot]);
            result[6 + frame * 2] = ma_atomic_float_get(&data->samples[slot + 1]);
        }
        if (ma_atomic_uint32_get(&data->sequence) != sequence) continue;
        result[2] = (float)(start & 0xffffff);
        result[3] = (float)(start >> 24);
        result[4] = end - data->last_read > CAPTURE_FRAMES;
        data->last_read = end;
        return 5 + (int)count * 2;
    }
    result[2] = result[3] = 0; result[4] = 1;
    return 5;
}
int dream_audio_output_control(void *handle, int command, double value) {
    player *p = handle; if (!p) return 0;
    stream_source *s = p->stream;
    if (s && (command == 3 || command == 5 || command == 6)) {
        if (command == 3) {
            if (value < 0) return 0;
            ma_atomic_uint64_set(&s->skip, ma_atomic_uint64_get(&s->write));
            s->position_base = (ma_uint64)value;
            ma_atomic_uint32_set(&s->eos, 0);
            ma_atomic_uint32_set(&s->starved, 0);
            ma_atomic_uint32_set(&s->ended, 0);
            ma_sound_seek_to_pcm_frame(p->sound, 0);
        } else if (command == 6) {
            ma_atomic_uint32_set(&s->eos, 1);
        }
        return 1;
    }
    switch (command) {
        case 1: return ma_sound_start(p->sound) == MA_SUCCESS;
        case 2: return ma_sound_stop(p->sound) == MA_SUCCESS;
        case 3:
            if (s) return 0;
            if (value < 0 || value > (double)p->buffer.ref.sizeInFrames) return 0;
            return ma_sound_seek_to_pcm_frame(p->sound, (ma_uint64)value) == MA_SUCCESS;
        case 4: ma_sound_set_volume(p->sound, (float)value); return 1;
        case 5: ma_sound_set_looping(p->sound, value != 0); return 1;
        default: return 0;
    }
}
double dream_audio_output_query(void *handle, int field) {
    player *p = handle; if (!p) return 0;
    stream_source *s = p->stream;
    if (s && field == 1) {
        return (double)(s->position_base + stream_read_index(s) - ma_atomic_uint64_get(&s->skip));
    }
    if (s && field == 2) return ma_atomic_uint32_get(&s->ended) != 0;
    if (s && field == 4) return (double)(ma_atomic_uint64_get(&s->write) - stream_read_index(s));
    if (s && field == 5) return ma_atomic_uint32_get(&s->starved) != 0 && ma_sound_is_playing(p->sound);
    if (field == 1) { ma_uint64 cursor = 0; ma_sound_get_cursor_in_pcm_frames(p->sound, &cursor); return (double)cursor; }
    if (field == 2) return ma_sound_at_end(p->sound);
    if (field == 3) return !ma_device_is_started(ma_engine_get_device(&p->out->engine));
    return ma_sound_is_playing(p->sound);
}
int dream_audio_output_sink(void *handle, int index) {
    player *p = handle; if (!p) return 0;
    output *next = acquire_output(index); if (!next) return 0;
    ma_sound *replacement = calloc(1, sizeof(*replacement));
    if (!replacement) { release_output(next); return 0; }
    if (ma_sound_init_from_data_source(&next->engine, p->source, MA_SOUND_FLAG_NO_SPATIALIZATION, NULL, replacement) != MA_SUCCESS) { free(replacement); release_output(next); return 0; }
    int playing = ma_sound_is_playing(p->sound);
    ma_sound_set_volume(replacement, ma_sound_get_volume(p->sound));
    ma_sound_set_looping(replacement, ma_sound_is_looping(p->sound));
    capture_node *next_tap = NULL;
    if (p->tap) {
        next_tap = attach_capture(next, replacement, p->tap->data);
        if (!next_tap) {
            ma_sound_uninit(replacement); free(replacement); release_output(next); return 0;
        }
    }
    ma_sound_stop(p->sound);
    ma_sound_uninit(p->sound);
    free(p->sound);
    if (p->tap) { ma_node_uninit(p->tap, NULL); free(p->tap); }
    if (next_tap) ma_atomic_uint64_set(&next_tap->data->writer, (ma_uint64)(ma_uintptr)next_tap);
    release_output(p->out);
    p->sound = replacement; p->out = next; p->tap = next_tap;
    if (playing) ma_sound_start(p->sound);
    return 1;
}
