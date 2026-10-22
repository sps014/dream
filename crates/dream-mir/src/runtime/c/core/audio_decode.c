/* Vendor assertions guard programmer contracts, not a guest-visible failure path. */
#define NDEBUG 1
#include "dream_core.h"
#include "dream_platform_internal.h"
#include <math.h>
#define DR_WAV_IMPLEMENTATION
#define DR_WAV_NO_STDIO
#define DR_MP3_IMPLEMENTATION
#define DR_MP3_NO_STDIO
#include "vendor/dr_libs/dr_wav.h"
#include "vendor/dr_libs/dr_mp3.h"

static void *audio_alloc(size_t size, void *user) {
    (void)user; return dream_platform_current->allocate(size);
}
static void *audio_resize(void *ptr, size_t size, void *user) {
    (void)user; return dream_platform_current->resize(ptr, size);
}
static void audio_free(void *ptr, void *user) {
    (void)user; dream_platform_current->deallocate(ptr);
}

/* Header words are sample rate and channel count, followed by interleaved f32 PCM. */
dream_ptr dream_audio_decode(dream_ptr source, int32_t max_bytes) {
    if (!source || dream_i32(source)[0] <= 0 || max_bytes < 4) return dream_array_new(0, 4);
    const void *bytes = (const char *)dream_array_bytes(source);
    size_t length = (size_t)dream_i32(source)[0];
    const unsigned char *magic = bytes;
    if (length >= 4 && magic[0] == 0x1a && magic[1] == 0x45 && magic[2] == 0xdf && magic[3] == 0xa3) {
        return dream_audio_decode_webm(source, max_bytes);
    }
    drwav *wav = dream_raw_calloc(1, sizeof(*wav));
    drmp3 *mp3 = dream_raw_calloc(1, sizeof(*mp3));
    if (!wav || !mp3) {
        dream_platform_current->deallocate(wav);
        dream_platform_current->deallocate(mp3);
        return dream_array_new(0, 4);
    }
    drwav_allocation_callbacks wa = {NULL, audio_alloc, audio_resize, audio_free};
    drmp3_allocation_callbacks ma = {NULL, audio_alloc, audio_resize, audio_free};
    int is_wav = drwav_init_memory(wav, bytes, length, &wa);
    int is_mp3 = !is_wav && drmp3_init_memory(mp3, bytes, length, &ma);
    if (!is_wav && !is_mp3) {
        dream_platform_current->deallocate(wav);
        dream_platform_current->deallocate(mp3);
        return dream_array_new(0, 4);
    }
    uint32_t channels = is_wav ? wav->channels : mp3->channels;
    uint32_t rate = is_wav ? wav->sampleRate : mp3->sampleRate;
    uint64_t frames = is_wav ? wav->totalPCMFrameCount : drmp3_get_pcm_frame_count(mp3);
    dream_ptr result;
    if (!rate || channels < 1 || channels > 2 || frames > (uint64_t)max_bytes / (channels * 4)) {
        result = dream_array_new(1, 4);
        *((float *)dream_array_bytes(result)) = -1;
    } else {
        result = dream_array_new((int32_t)(frames * channels + 2), 4);
        float *out = (float *)dream_array_bytes(result);
        out[0] = (float)rate; out[1] = (float)channels;
        uint64_t read = is_wav ? drwav_read_pcm_frames_f32(wav, frames, out + 2)
                              : drmp3_read_pcm_frames_f32(mp3, frames, out + 2);
        if (read != frames) {
            dream_release(result);
            result = dream_array_new(0, 4);
        }
    }
    if (is_wav) drwav_uninit(wav); else drmp3_uninit(mp3);
    dream_platform_current->deallocate(wav);
    dream_platform_current->deallocate(mp3);
    return result;
}
