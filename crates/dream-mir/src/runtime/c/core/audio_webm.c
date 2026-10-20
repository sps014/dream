#define NDEBUG 1
#include "dream_core.h"
#include "dream_platform_internal.h"
#include <stdlib.h>
#include <math.h>
#include "vendor/opus/include/opus.h"

/* Nestegg's temporary allocations use the instance's platform, like the other codecs. */
#define malloc(n) dream_platform_current->allocate(n)
#define calloc(n, s) dream_raw_calloc(n, s)
#define realloc(p, n) dream_platform_current->resize(p, n)
#define free(p) dream_platform_current->deallocate(p)
#include "vendor/nestegg/src/nestegg.c"
#undef malloc
#undef calloc
#undef realloc
#undef free

typedef struct { const unsigned char *bytes; int64_t length; int64_t position; } webm_input;
static int64_t webm_read(void *destination, size_t length, void *userdata) {
    webm_input *input = userdata;
    size_t available = (size_t)(input->length - input->position);
    if (length > available) length = available;
    memcpy(destination, input->bytes + input->position, length);
    input->position += (int64_t)length;
    return (int64_t)length;
}
static int webm_seek(int64_t offset, int whence, void *userdata) {
    webm_input *input = userdata;
    int64_t base = whence == NESTEGG_SEEK_SET ? 0 : whence == NESTEGG_SEEK_CUR ? input->position : input->length;
    if (offset < -base || offset > input->length - base) return -1;
    input->position = base + offset;
    return 0;
}
static int64_t webm_tell(void *userdata) { return ((webm_input *)userdata)->position; }

/* Only mono/stereo mapping family zero is accepted; video and other tracks are skipped. */
dream_ptr dream_audio_decode_webm(dream_ptr source, int32_t max_bytes) {
    webm_input input = { (const unsigned char *)dream_p(source) + 4, dream_i32(source)[0], 0 };
    nestegg_io io = { webm_read, webm_seek, webm_tell, &input };
    nestegg *context = NULL;
    OpusDecoder *decoder = NULL;
    nestegg_packet *packet = NULL;
    float *pcm = NULL;
    float *frame = NULL;
    size_t count = 0, capacity = 0;
    int failure = 0, status = -1, channels = 0;
    unsigned int tracks = 0, track = 0;
    uint64_t skip = 0;
    if (nestegg_init(&context, io, NULL, input.length) != 0) goto done;
    if (nestegg_track_count(context, &tracks) != 0) goto done;
    for (; track < tracks; ++track) {
        if (nestegg_track_type(context, track) == NESTEGG_TRACK_AUDIO &&
            nestegg_track_codec_id(context, track) == NESTEGG_CODEC_OPUS) break;
    }
    if (track == tracks) goto done;
    nestegg_audio_params parameters;
    if (nestegg_track_audio_params(context, track, &parameters) != 0) goto done;
    channels = (int)parameters.channels;
    if (channels < 1 || channels > 2) goto done;
    unsigned char *head = NULL; size_t head_length = 0;
    if (nestegg_track_codec_data(context, track, 0, &head, &head_length) != 0 ||
        head_length < 19 || memcmp(head, "OpusHead", 8) != 0 || head[9] != channels || head[18] != 0) goto done;
    skip = (uint64_t)head[10] | (uint64_t)head[11] << 8;
    if (parameters.codec_delay) {
        double delayed = (double)parameters.codec_delay * 48000.0 / 1000000000.0;
        if (delayed > (double)UINT32_MAX) goto done;
        skip = (uint64_t)(delayed + 0.5);
    }
    int error = 0;
    decoder = opus_decoder_create(48000, channels, &error);
    if (!decoder || error != OPUS_OK) goto done;
    int gain = (int16_t)((unsigned)head[16] | (unsigned)head[17] << 8);
    if (opus_decoder_ctl(decoder, OPUS_SET_GAIN(gain)) != OPUS_OK) goto done;
    size_t maximum = (size_t)max_bytes / sizeof(float);
    frame = dream_platform_current->allocate(5760 * 2 * sizeof(float));
    if (!frame) { failure = -2; goto done; }
    while ((status = nestegg_read_packet(context, &packet)) > 0) {
        unsigned int packet_track = 0, chunks = 0;
        if (nestegg_packet_track(packet, &packet_track) != 0) goto done;
        if (packet_track == track) {
            size_t packet_start = count;
            if (nestegg_packet_count(packet, &chunks) != 0) goto done;
            for (unsigned int chunk = 0; chunk < chunks; ++chunk) {
                unsigned char *data = NULL; size_t length = 0;
                if (nestegg_packet_data(packet, chunk, &data, &length) != 0 || length > INT32_MAX) goto done;
                int frames = opus_decode_float(decoder, data, (opus_int32)length, frame, 5760, 0);
                if (frames < 0) goto done;
                size_t discard = skip < (uint64_t)frames ? (size_t)skip : (size_t)frames;
                skip -= discard;
                size_t samples = ((size_t)frames - discard) * (size_t)channels;
                if (samples > maximum - count) { failure = -1; goto done; }
                size_t needed = count + samples;
                if (needed > capacity) {
                    size_t next = capacity ? capacity * 2 : 4096;
                    if (next < needed) next = needed;
                    if (next > maximum) next = maximum;
                    float *grown = dream_platform_current->resize(pcm, next * sizeof(float));
                    if (!grown) { failure = -2; goto done; }
                    pcm = grown; capacity = next;
                }
                memcpy(pcm + count, frame + discard * (size_t)channels, samples * sizeof(float));
                count += samples;
            }
            int64_t padding = 0;
            if (nestegg_packet_discard_padding(packet, &padding) == 0 && padding) {
                double trim = fabs((double)padding) * 48000.0 / 1000000000.0;
                size_t packet_frames = (count - packet_start) / (size_t)channels;
                if (trim > (double)packet_frames + 0.5) goto done;
                size_t remove = (size_t)(trim + 0.5) * (size_t)channels;
                if (remove > count - packet_start) goto done;
                if (padding < 0) memmove(pcm + packet_start, pcm + packet_start + remove,
                    (count - packet_start - remove) * sizeof(float));
                count -= remove;
            }
        }
        nestegg_free_packet(packet); packet = NULL;
    }
    if (status == 0) failure = 1;
done:
    if (packet) nestegg_free_packet(packet);
    if (decoder) opus_decoder_destroy(decoder);
    if (context) nestegg_destroy(context);
    dream_ptr result;
    if (failure == 1) {
        result = dream_array_new((int32_t)count + 2, 4);
        float *out = (float *)((char *)dream_p(result) + 4);
        out[0] = 48000; out[1] = (float)channels;
        if (count) memcpy(out + 2, pcm, count * sizeof(float));
    } else {
        result = dream_array_new(failure < 0 ? 1 : 0, 4);
        if (failure < 0) *((float *)((char *)dream_p(result) + 4)) = (float)failure;
    }
    dream_platform_current->deallocate(pcm);
    dream_platform_current->deallocate(frame);
    return result;
}
