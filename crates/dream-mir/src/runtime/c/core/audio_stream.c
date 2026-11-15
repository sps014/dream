/* Vendor assertions guard programmer contracts, not a guest-visible failure path. */
#define NDEBUG 1
#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_audio.h"
#include <math.h>
#include <string.h>
#include "vendor/dr_libs/dr_wav.h"
#include "vendor/dr_libs/dr_mp3.h"
#include "nestegg/nestegg.h"
#include "vendor/opus/include/opus.h"

/*
 * Incremental decoding over a bounded compressed queue. The vendored decoders pull
 * synchronously and treat a short read as end of stream, so a decoder is only entered
 * when the queue holds its lookahead, the input has ended, or the queue is full. A
 * starved read with input still pending is reported as ELIMIT: the encoded element
 * needed more bytes than the queue could provide.
 */
#define STREAM_INIT_LOOKAHEAD (256u * 1024u)
#define STREAM_PCM_LOOKAHEAD (128u * 1024u)
#define STREAM_WEBM_LOOKAHEAD (2u * 1024u * 1024u)
/* Nestegg rewinds within its 8 KiB read buffer; decoders never seek further back after init. */
#define STREAM_RETAIN (64u * 1024u)
#define STREAM_BLOCK_FRAMES 4096u
#define OPUS_MAX_FRAMES 5760u

enum { FORMAT_NONE, FORMAT_WAV, FORMAT_MP3, FORMAT_WEBM };
enum { STATUS_NEED = 0, STATUS_DATA = 1, STATUS_END = 2,
       STATUS_FORMAT = -1, STATUS_MEMORY = -2, STATUS_CHANNELS = -3, STATUS_LIMIT = -4 };

typedef struct {
    unsigned char *data;
    size_t allocated, capacity, length;
    uint64_t base, cursor, id3_skip;
    int id3_checked, eof, starved, format, status, channels;
    uint32_t rate;
    double duration;
    drwav *wav;
    drmp3 *mp3;
    nestegg *webm;
    OpusDecoder *opus;
    unsigned int track;
    uint64_t opus_skip;
    float *pending, *out;
    size_t pending_allocated, pending_count, pending_offset, out_allocated;
} audio_stream;

static void *stream_alloc(size_t size, void *user) {
    (void)user; return dream_platform_current->allocate(size);
}
static void *stream_resize(void *ptr, size_t size, void *user) {
    (void)user; return dream_platform_current->resize(ptr, size);
}
static void stream_free(void *ptr, void *user) {
    (void)user; dream_platform_current->deallocate(ptr);
}

static size_t stream_ahead(const audio_stream *s) {
    return (size_t)(s->base + s->length - s->cursor);
}
static size_t stream_retain(const audio_stream *s) {
    return s->capacity / 4 < STREAM_RETAIN ? s->capacity / 4 : STREAM_RETAIN;
}
/* Detection may rewind to offset zero, so nothing is discarded until a decoder is open. */
static size_t stream_discardable(const audio_stream *s) {
    if (s->format == FORMAT_NONE) return 0;
    uint64_t behind = s->cursor - s->base;
    return behind > stream_retain(s) ? (size_t)(behind - stream_retain(s)) : 0;
}
static size_t stream_space(const audio_stream *s) {
    return s->capacity - s->length + stream_discardable(s);
}
static void stream_compact(audio_stream *s) {
    size_t drop = stream_discardable(s);
    if (!drop) return;
    memmove(s->data, s->data + drop, s->length - drop);
    s->length -= drop;
    s->base += drop;
}
static int stream_ready(const audio_stream *s, size_t lookahead) {
    return s->eof || stream_ahead(s) >= lookahead || stream_space(s) == 0;
}
static int grow_floats(float **buffer, size_t *allocated, size_t needed) {
    if (needed <= *allocated) return 1;
    size_t next = *allocated ? *allocated * 2 : 8192;
    if (next < needed) next = needed;
    float *grown = dream_platform_current->resize(*buffer, next * sizeof(float));
    if (!grown) return 0;
    *buffer = grown;
    *allocated = next;
    return 1;
}

/* ID3v2 artwork can exceed the queue; tags are dropped as they arrive, before detection. */
static void stream_check_id3(audio_stream *s) {
    if (s->id3_checked || s->format != FORMAT_NONE || (s->length < 10 && !s->eof)) return;
    s->id3_checked = 1;
    const unsigned char *m = s->data;
    if (s->length < 10 || m[0] != 'I' || m[1] != 'D' || m[2] != '3') return;
    uint64_t size = 10 + ((uint64_t)(m[6] & 0x7f) << 21 | (uint64_t)(m[7] & 0x7f) << 14 |
                          (uint64_t)(m[8] & 0x7f) << 7 | (uint64_t)(m[9] & 0x7f));
    if (m[5] & 0x10) size += 10;
    size_t drop = size < s->length ? (size_t)size : s->length;
    memmove(s->data, s->data + drop, s->length - drop);
    s->length -= drop;
    s->id3_skip = size - drop;
}

static size_t stream_read_bytes(audio_stream *s, void *out, size_t n) {
    size_t ahead = stream_ahead(s);
    if (n > ahead) {
        if (!s->eof) s->starved = 1;
        n = ahead;
    }
    memcpy(out, s->data + (size_t)(s->cursor - s->base), n);
    s->cursor += n;
    return n;
}
/* Origin: 0 start, 1 current, 2 end. The end is unknown until the input finishes. */
static int stream_seek_to(audio_stream *s, int64_t offset, int origin) {
    int64_t end = (int64_t)(s->base + s->length), target;
    if (origin == 0) target = offset;
    else if (origin == 1) target = (int64_t)s->cursor + offset;
    else if (s->eof) target = end + offset;
    else return 0;
    if (target < (int64_t)s->base) return 0;
    if (target > end) {
        if (!s->eof) s->starved = 1;
        return 0;
    }
    s->cursor = (uint64_t)target;
    return 1;
}

static size_t wav_read(void *user, void *out, size_t n) { return stream_read_bytes(user, out, n); }
static drwav_bool32 wav_seek(void *user, int offset, drwav_seek_origin origin) {
    return stream_seek_to(user, offset, origin == DRWAV_SEEK_SET ? 0 : origin == DRWAV_SEEK_CUR ? 1 : 2);
}
static drwav_bool32 wav_tell(void *user, drwav_int64 *cursor) {
    *cursor = (drwav_int64)((audio_stream *)user)->cursor;
    return DRWAV_TRUE;
}
static size_t mp3_read(void *user, void *out, size_t n) { return stream_read_bytes(user, out, n); }
static drmp3_bool32 mp3_seek(void *user, int offset, drmp3_seek_origin origin) {
    return stream_seek_to(user, offset, origin == DRMP3_SEEK_SET ? 0 : origin == DRMP3_SEEK_CUR ? 1 : 2);
}
static drmp3_bool32 mp3_tell(void *user, drmp3_int64 *cursor) {
    *cursor = (drmp3_int64)((audio_stream *)user)->cursor;
    return DRMP3_TRUE;
}
static int64_t webm_read(void *buffer, size_t length, void *user) {
    return (int64_t)stream_read_bytes(user, buffer, length);
}
static int webm_seek(int64_t offset, int whence, void *user) {
    int origin = whence == NESTEGG_SEEK_SET ? 0 : whence == NESTEGG_SEEK_CUR ? 1 : 2;
    return stream_seek_to(user, offset, origin) ? 0 : -1;
}
static int64_t webm_tell(void *user) { return (int64_t)((audio_stream *)user)->cursor; }

static int stream_failure(const audio_stream *s) {
    return s->starved && !s->eof ? STATUS_LIMIT : STATUS_FORMAT;
}

/* Matches the buffered decoder: first Opus track, mono/stereo, mapping family zero. */
static int webm_open(audio_stream *s) {
    nestegg_io io = { webm_read, webm_seek, webm_tell, s };
    nestegg *context = NULL;
    if (nestegg_init(&context, io, NULL, -1) != 0) return stream_failure(s);
    s->webm = context;
    unsigned int tracks = 0, track = 0;
    if (nestegg_track_count(context, &tracks) != 0) return STATUS_FORMAT;
    for (; track < tracks; ++track) {
        if (nestegg_track_type(context, track) == NESTEGG_TRACK_AUDIO &&
            nestegg_track_codec_id(context, track) == NESTEGG_CODEC_OPUS) break;
    }
    if (track == tracks) return STATUS_FORMAT;
    nestegg_audio_params parameters;
    if (nestegg_track_audio_params(context, track, &parameters) != 0) return STATUS_FORMAT;
    int channels = (int)parameters.channels;
    if (channels < 1 || channels > 2) return STATUS_CHANNELS;
    unsigned char *head = NULL;
    size_t head_length = 0;
    if (nestegg_track_codec_data(context, track, 0, &head, &head_length) != 0 || head_length < 19 ||
        memcmp(head, "OpusHead", 8) != 0 || head[9] != channels || head[18] != 0) return STATUS_FORMAT;
    s->opus_skip = (uint64_t)head[10] | (uint64_t)head[11] << 8;
    if (parameters.codec_delay) {
        double delayed = (double)parameters.codec_delay * 48000.0 / 1000000000.0;
        if (delayed > (double)UINT32_MAX) return STATUS_FORMAT;
        s->opus_skip = (uint64_t)(delayed + 0.5);
    }
    int error = 0;
    s->opus = opus_decoder_create(48000, channels, &error);
    if (!s->opus || error != OPUS_OK) return STATUS_FORMAT;
    int gain = (int16_t)((unsigned)head[16] | (unsigned)head[17] << 8);
    if (opus_decoder_ctl(s->opus, OPUS_SET_GAIN(gain)) != OPUS_OK) return STATUS_FORMAT;
    uint64_t nanoseconds = 0;
    if (nestegg_duration(context, &nanoseconds) == 0) s->duration = (double)nanoseconds / 1e9;
    s->track = track;
    s->channels = channels;
    s->rate = 48000;
    s->format = FORMAT_WEBM;
    return STATUS_DATA;
}

static int stream_open_decoder(audio_stream *s) {
    const unsigned char *m = s->data;
    if (s->length >= 4 && m[0] == 0x1a && m[1] == 0x45 && m[2] == 0xdf && m[3] == 0xa3) return webm_open(s);
    int riff = s->length >= 4 && (!memcmp(m, "RIFF", 4) || !memcmp(m, "RF64", 4) ||
                                  !memcmp(m, "riff", 4) || !memcmp(m, "BW64", 4));
    if (riff) {
        drwav_allocation_callbacks callbacks = {NULL, stream_alloc, stream_resize, stream_free};
        s->wav = dream_raw_calloc(1, sizeof(*s->wav));
        if (!s->wav) return STATUS_MEMORY;
        if (!drwav_init_ex(s->wav, wav_read, wav_seek, wav_tell, NULL, s, NULL, DRWAV_SEQUENTIAL, &callbacks)) {
            dream_platform_current->deallocate(s->wav);
            s->wav = NULL;
            return stream_failure(s);
        }
        s->format = FORMAT_WAV;
        s->rate = s->wav->sampleRate;
        s->channels = s->wav->channels;
        if (s->wav->dataChunkDataSize && s->wav->dataChunkDataSize != 0xffffffffu && s->rate) {
            s->duration = (double)s->wav->totalPCMFrameCount / (double)s->rate;
        }
    } else {
        drmp3_allocation_callbacks callbacks = {NULL, stream_alloc, stream_resize, stream_free};
        s->mp3 = dream_raw_calloc(1, sizeof(*s->mp3));
        if (!s->mp3) return STATUS_MEMORY;
        if (!drmp3_init(s->mp3, mp3_read, mp3_seek, mp3_tell, NULL, s, &callbacks)) {
            dream_platform_current->deallocate(s->mp3);
            s->mp3 = NULL;
            return stream_failure(s);
        }
        s->format = FORMAT_MP3;
        s->rate = s->mp3->sampleRate;
        s->channels = (int)s->mp3->channels;
        if (s->mp3->totalPCMFrameCount != DRMP3_UINT64_MAX && s->rate) {
            s->duration = (double)s->mp3->totalPCMFrameCount / (double)s->rate;
        }
    }
    if (s->starved && !s->eof) return STATUS_LIMIT;
    if (!s->rate) return STATUS_FORMAT;
    if (s->channels < 1 || s->channels > 2) return STATUS_CHANNELS;
    return STATUS_DATA;
}

static int webm_decode(audio_stream *s) {
    nestegg_packet *packet = NULL;
    int read = nestegg_read_packet(s->webm, &packet);
    if (s->starved && !s->eof) {
        if (read > 0) nestegg_free_packet(packet);
        return STATUS_LIMIT;
    }
    if (read == 0) return STATUS_END;
    if (read < 0) return STATUS_FORMAT;
    int result = STATUS_FORMAT;
    unsigned int packet_track = 0, chunks = 0;
    size_t channels = (size_t)s->channels, count = 0;
    if (nestegg_packet_track(packet, &packet_track) != 0) goto done;
    if (packet_track != s->track) { result = STATUS_DATA; goto done; }
    if (nestegg_packet_count(packet, &chunks) != 0) goto done;
    for (unsigned int chunk = 0; chunk < chunks; ++chunk) {
        unsigned char *data = NULL;
        size_t length = 0;
        if (nestegg_packet_data(packet, chunk, &data, &length) != 0 || length > INT32_MAX) goto done;
        if (!grow_floats(&s->pending, &s->pending_allocated, count + OPUS_MAX_FRAMES * channels)) {
            result = STATUS_MEMORY;
            goto done;
        }
        float *frame = s->pending + count;
        int frames = opus_decode_float(s->opus, data, (opus_int32)length, frame, OPUS_MAX_FRAMES, 0);
        if (frames < 0) goto done;
        size_t discard = s->opus_skip < (uint64_t)frames ? (size_t)s->opus_skip : (size_t)frames;
        s->opus_skip -= discard;
        size_t samples = ((size_t)frames - discard) * channels;
        memmove(frame, frame + discard * channels, samples * sizeof(float));
        count += samples;
    }
    int64_t padding = 0;
    if (nestegg_packet_discard_padding(packet, &padding) == 0 && padding) {
        double trim = fabs((double)padding) * 48000.0 / 1000000000.0;
        if (trim > (double)(count / channels) + 0.5) goto done;
        size_t remove = (size_t)(trim + 0.5) * channels;
        if (remove > count) goto done;
        if (padding < 0) memmove(s->pending, s->pending + remove, (count - remove) * sizeof(float));
        count -= remove;
    }
    s->pending_count = count;
    result = STATUS_DATA;
done:
    nestegg_free_packet(packet);
    return result;
}

static int stream_decode(audio_stream *s) {
    if (s->format == FORMAT_WEBM) return webm_decode(s);
    size_t channels = (size_t)s->channels;
    if (!grow_floats(&s->pending, &s->pending_allocated, STREAM_BLOCK_FRAMES * channels)) return STATUS_MEMORY;
    uint64_t frames = s->format == FORMAT_WAV
        ? drwav_read_pcm_frames_f32(s->wav, STREAM_BLOCK_FRAMES, s->pending)
        : drmp3_read_pcm_frames_f32(s->mp3, STREAM_BLOCK_FRAMES, s->pending);
    if (s->starved && !s->eof) return STATUS_LIMIT;
    if (frames == 0) return STATUS_END;
    s->pending_count = (size_t)frames * channels;
    return STATUS_DATA;
}

uintptr_t dream_audio_stream_open(int32_t capacity) {
    if (capacity <= 0) return 0;
    audio_stream *s = dream_raw_calloc(1, sizeof(*s));
    if (!s) return 0;
    s->capacity = (size_t)capacity;
    s->duration = -1.0;
    return (uintptr_t)s;
}

/* Bytes the next push accepts; ID3 bytes still being skipped are never stored. */
int32_t dream_audio_stream_space(uintptr_t handle) {
    audio_stream *s = (audio_stream *)handle;
    if (!s || s->eof || s->status < 0) return 0;
    uint64_t space = (uint64_t)stream_space(s) + s->id3_skip;
    return space > INT32_MAX ? INT32_MAX : (int32_t)space;
}

int32_t dream_audio_stream_push(uintptr_t handle, const unsigned char *bytes, size_t length) {
    audio_stream *s = (audio_stream *)handle;
    if (!s || (!bytes && length) || s->eof) return 0;
    size_t skip = s->id3_skip < length ? (size_t)s->id3_skip : length;
    if (length - skip > stream_space(s)) return 0;
    bytes += skip;
    length -= skip;
    s->id3_skip -= skip;
    stream_compact(s);
    if (s->length + length > s->allocated) {
        size_t next = s->allocated ? s->allocated * 2 : 65536;
        if (next < s->length + length) next = s->length + length;
        if (next > s->capacity) next = s->capacity;
        unsigned char *grown = dream_platform_current->resize(s->data, next);
        if (!grown) return 0;
        s->data = grown;
        s->allocated = next;
    }
    if (length) memcpy(s->data + s->length, bytes, length);
    s->length += length;
    stream_check_id3(s);
    return 1;
}

void dream_audio_stream_finish(uintptr_t handle) {
    audio_stream *s = (audio_stream *)handle;
    if (!s) return;
    s->eof = 1;
    stream_check_id3(s);
}

/* The chunk borrows decoder memory until the next decode or close. */
dream_audio_chunk dream_audio_stream_decode(uintptr_t handle, int32_t max_frames) {
    audio_stream *s = (audio_stream *)handle;
    size_t produced = 0;
    int status = STATUS_FORMAT;
    if (!s) goto result;
    if (max_frames < 1) max_frames = 1;
    if (max_frames > 65536) max_frames = 65536;
    if (s->format == FORMAT_NONE && s->status == STATUS_NEED) {
        if (!stream_ready(s, STREAM_INIT_LOOKAHEAD)) { status = STATUS_NEED; goto result; }
        int opened = stream_open_decoder(s);
        if (opened < 0) s->status = opened;
    }
    if (s->status >= 0) {
        size_t channels = (size_t)s->channels, want = (size_t)max_frames * channels;
        size_t lookahead = s->format == FORMAT_WEBM ? STREAM_WEBM_LOOKAHEAD : STREAM_PCM_LOOKAHEAD;
        if (!grow_floats(&s->out, &s->out_allocated, want)) s->status = STATUS_MEMORY;
        while (s->status >= 0 && produced < want) {
            if (s->pending_offset < s->pending_count) {
                size_t n = s->pending_count - s->pending_offset;
                if (n > want - produced) n = want - produced;
                memcpy(s->out + produced, s->pending + s->pending_offset, n * sizeof(float));
                s->pending_offset += n;
                produced += n;
                continue;
            }
            if (s->status == STATUS_END || !stream_ready(s, lookahead)) break;
            s->pending_offset = s->pending_count = 0;
            int decoded = stream_decode(s);
            if (decoded != STATUS_DATA) s->status = decoded;
        }
        stream_compact(s);
    }
    status = produced ? STATUS_DATA : s->status;
result:;
    dream_audio_chunk chunk = {status, s ? (int32_t)s->rate : 0, s ? s->channels : 0,
                               produced ? s->out : NULL, produced};
    return chunk;
}

/* Seconds when the container declares a length; negative when unknown. */
double dream_audio_stream_duration(uintptr_t handle) {
    audio_stream *s = (audio_stream *)handle;
    return s ? s->duration : -1.0;
}

/* dr_wav supplies the validated data chunk and frame alignment. Compressed WAV
 * needs codec checkpoints; accepting only independently decodable frames avoids
 * a seek that appears successful but reconstructs the wrong predictor state. */
dream_audio_layout dream_audio_stream_layout(uintptr_t handle) {
    audio_stream *s = (audio_stream *)handle;
    dream_audio_layout layout = {0, 0, 0};
    if (!s || !s->wav || (s->wav->translatedFormatTag != DR_WAVE_FORMAT_PCM &&
                          s->wav->translatedFormatTag != DR_WAVE_FORMAT_IEEE_FLOAT) ||
        !s->wav->fmt.blockAlign || s->wav->dataChunkDataPos > INT64_MAX) return layout;
    layout.data_position = s->wav->dataChunkDataPos;
    layout.block_align = s->wav->fmt.blockAlign;
    layout.frames = s->wav->totalPCMFrameCount;
    return layout;
}

int64_t dream_audio_layout_offset(dream_audio_layout layout, int64_t frame) {
    if (!layout.block_align || frame < 0 || (uint64_t)frame > layout.frames ||
        (uint64_t)frame > (INT64_MAX - layout.data_position) / layout.block_align) return -1;
    return (int64_t)(layout.data_position + (uint64_t)frame * layout.block_align);
}

int32_t dream_audio_stream_reposition(uintptr_t handle, int64_t frame) {
    audio_stream *s = (audio_stream *)handle;
    int64_t offset = dream_audio_layout_offset(dream_audio_stream_layout(handle), frame);
    if (offset < 0) return 0;
    s->base = s->cursor = (uint64_t)offset;
    s->length = 0;
    s->eof = s->starved = 0;
    s->status = STATUS_DATA;
    s->pending_count = s->pending_offset = 0;
    s->wav->readCursorInPCMFrames = (uint64_t)frame;
    s->wav->bytesRemaining = s->wav->dataChunkDataSize -
        (uint64_t)frame * s->wav->fmt.blockAlign;
    return 1;
}

void dream_audio_stream_close(uintptr_t handle) {
    audio_stream *s = (audio_stream *)handle;
    if (!s) return;
    if (s->wav) { drwav_uninit(s->wav); dream_platform_current->deallocate(s->wav); }
    if (s->mp3) { drmp3_uninit(s->mp3); dream_platform_current->deallocate(s->mp3); }
    if (s->opus) opus_decoder_destroy(s->opus);
    if (s->webm) nestegg_destroy(s->webm);
    dream_platform_current->deallocate(s->data);
    dream_platform_current->deallocate(s->pending);
    dream_platform_current->deallocate(s->out);
    dream_platform_current->deallocate(s);
}
