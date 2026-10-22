/* Vendor assertions guard programmer contracts, not a guest-visible failure path. */
#define NDEBUG 1
#include "dream_core.h"
#include "dream_platform_internal.h"
#include "dream_audio.h"
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

float *dream_audio_words_status(float status, size_t *count) {
    float *words = dream_platform_current->allocate(sizeof(float));
    *count = words ? 1 : 0;
    if (words) words[0] = status;
    return words;
}

/* Decoding touches only platform memory and the caller's bytes, so it may run on any thread. */
float *dream_audio_decode_words(const unsigned char *bytes, size_t length, int32_t max_bytes,
                                size_t *count) {
    *count = 0;
    if (!bytes || !length || max_bytes < 4) return NULL;
    if (length >= 4 && bytes[0] == 0x1a && bytes[1] == 0x45 && bytes[2] == 0xdf && bytes[3] == 0xa3) {
        return dream_audio_decode_webm_words(bytes, length, max_bytes, count);
    }
    drwav *wav = dream_raw_calloc(1, sizeof(*wav));
    drmp3 *mp3 = dream_raw_calloc(1, sizeof(*mp3));
    if (!wav || !mp3) {
        dream_platform_current->deallocate(wav);
        dream_platform_current->deallocate(mp3);
        return dream_audio_words_status(-2, count);
    }
    drwav_allocation_callbacks wa = {NULL, audio_alloc, audio_resize, audio_free};
    drmp3_allocation_callbacks ma = {NULL, audio_alloc, audio_resize, audio_free};
    int is_wav = drwav_init_memory(wav, bytes, length, &wa);
    int is_mp3 = !is_wav && drmp3_init_memory(mp3, bytes, length, &ma);
    if (!is_wav && !is_mp3) {
        dream_platform_current->deallocate(wav);
        dream_platform_current->deallocate(mp3);
        return NULL;
    }
    uint32_t channels = is_wav ? wav->channels : mp3->channels;
    uint32_t rate = is_wav ? wav->sampleRate : mp3->sampleRate;
    uint64_t frames = is_wav ? wav->totalPCMFrameCount : drmp3_get_pcm_frame_count(mp3);
    float *words = NULL;
    if (!rate || channels < 1 || channels > 2 || frames > (uint64_t)max_bytes / (channels * 4)) {
        words = dream_audio_words_status(-1, count);
    } else {
        size_t total = (size_t)(frames * channels + 2);
        words = dream_platform_current->allocate(total * sizeof(float));
        if (!words) {
            words = dream_audio_words_status(-2, count);
        } else {
            words[0] = (float)rate; words[1] = (float)channels;
            uint64_t read = is_wav ? drwav_read_pcm_frames_f32(wav, frames, words + 2)
                                  : drmp3_read_pcm_frames_f32(mp3, frames, words + 2);
            if (read != frames) {
                dream_platform_current->deallocate(words);
                words = NULL;
            } else {
                *count = total;
            }
        }
    }
    if (is_wav) drwav_uninit(wav); else drmp3_uninit(mp3);
    dream_platform_current->deallocate(wav);
    dream_platform_current->deallocate(mp3);
    return words;
}
