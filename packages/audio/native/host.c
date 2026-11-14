/* Guest-facing output handles. output.c assumes these entry points are serialized;
 * miniaudio runs its own mixing threads. */
#include <dream_embed.h>

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
static CRITICAL_SECTION players_lock;
static INIT_ONCE players_once = INIT_ONCE_STATIC_INIT;
static BOOL CALLBACK players_init(PINIT_ONCE once, PVOID param, PVOID *context) {
    (void)once;
    (void)param;
    (void)context;
    InitializeCriticalSection(&players_lock);
    return TRUE;
}
static void lock_players(void) {
    InitOnceExecuteOnce(&players_once, players_init, NULL, NULL);
    EnterCriticalSection(&players_lock);
}
static void unlock_players(void) { LeaveCriticalSection(&players_lock); }
#else
#include <pthread.h>
static pthread_mutex_t players_lock = PTHREAD_MUTEX_INITIALIZER;
static void lock_players(void) { pthread_mutex_lock(&players_lock); }
static void unlock_players(void) { pthread_mutex_unlock(&players_lock); }
#endif

#define SLOT_BITS 23u
#define SLOT_MASK ((1u << SLOT_BITS) - 1u)
#define MAX_LIVE SLOT_MASK

typedef struct {
    uint8_t generation;
    void *player;
} audio_slot;

static audio_slot *slots;
static uint32_t slot_count;
static uint32_t *free_slots;
static uint32_t free_count;
static uint32_t free_cap;

void *dream_audio_output_create(const float *samples, int count, int rate, int channels);
void *dream_audio_output_stream(int rate, int channels, int frames);
int dream_audio_output_write(void *handle, const float *samples, int count);
void dream_audio_output_close(void *handle);
int dream_audio_output_capture(void *handle, int frames, float *result);
int dream_audio_output_control(void *handle, int command, double value);
double dream_audio_output_query(void *handle, int field);
int dream_audio_output_sink(void *handle, int index);
void dream_audio_context_idle(void);
int dream_audio_device_count(void);
const char *dream_audio_device_name(int index);
int dream_audio_device_default(int index);

static int32_t handle_of(uint32_t slot, uint8_t generation) {
    return (int32_t)(((uint32_t)generation << SLOT_BITS) | slot);
}

static void *player_get(int32_t handle) {
    if (handle <= 0) return NULL;
    uint32_t raw = (uint32_t)handle;
    uint32_t index = raw & SLOT_MASK;
    uint8_t generation = (uint8_t)(raw >> SLOT_BITS);
    if (index >= slot_count) return NULL;
    audio_slot *slot = &slots[index];
    if (slot->generation != generation || !slot->player) return NULL;
    return slot->player;
}

static int free_push(uint32_t index) {
    if (free_count == free_cap) {
        uint32_t cap = free_cap ? free_cap * 2 : 8;
        uint32_t *grown = realloc(free_slots, (size_t)cap * sizeof(*free_slots));
        if (!grown) return 0;
        free_slots = grown;
        free_cap = cap;
    }
    free_slots[free_count++] = index;
    return 1;
}

static int32_t player_insert(void *player) {
    if (!player) return 0;
    uint32_t index;
    if (free_count > 0) {
        index = free_slots[--free_count];
    } else if (slot_count < MAX_LIVE) {
        audio_slot *grown = realloc(slots, (size_t)(slot_count + 1) * sizeof(*slots));
        if (!grown) {
            dream_audio_output_close(player);
            return 0;
        }
        slots = grown;
        index = slot_count++;
        slots[index].generation = 1;
        slots[index].player = NULL;
    } else {
        dream_audio_output_close(player);
        return 0;
    }
    slots[index].player = player;
    return handle_of(index, slots[index].generation);
}

static void player_remove(int32_t handle) {
    void *player = player_get(handle);
    if (!player) return;
    uint32_t index = (uint32_t)handle & SLOT_MASK;
    dream_audio_output_close(player);
    slots[index].player = NULL;
    slots[index].generation = slots[index].generation == 255 ? 1 : (uint8_t)(slots[index].generation + 1);
    if (!free_push(index)) slots[index].generation = (uint8_t)((uint32_t)handle >> SLOT_BITS);
}

typedef struct {
    char *data;
    size_t len;
    size_t cap;
} text;

static int text_add(text *out, const char *bytes, size_t count) {
    if (out->len + count + 1 > out->cap) {
        size_t cap = out->cap ? out->cap : 64;
        while (out->len + count + 1 > cap) cap *= 2;
        char *grown = realloc(out->data, cap);
        if (!grown) return 0;
        out->data = grown;
        out->cap = cap;
    }
    memcpy(out->data + out->len, bytes, count);
    out->len += count;
    out->data[out->len] = 0;
    return 1;
}

static int text_json_string(text *out, const char *value) {
    if (!text_add(out, "\"", 1)) return 0;
    for (const unsigned char *cursor = (const unsigned char *)(value ? value : ""); *cursor; cursor++) {
        char escaped[8];
        const char *piece = (const char *)cursor;
        size_t count = 1;
        if (*cursor == '"' || *cursor == '\\') {
            escaped[0] = '\\';
            escaped[1] = (char)*cursor;
            piece = escaped;
            count = 2;
        } else if (*cursor < 0x20) {
            snprintf(escaped, sizeof(escaped), "\\u%04x", *cursor);
            piece = escaped;
            count = 6;
        }
        if (!text_add(out, piece, count)) return 0;
    }
    return text_add(out, "\"", 1);
}

static int append_device(text *out, int index, int first) {
    const char *name = dream_audio_device_name(index);
    if (!name) return 1;
    char id[1024];
    snprintf(id, sizeof(id), "native:%d:%s", index, name);
    if (!first && !text_add(out, ",", 1)) return 0;
    if (!text_add(out, "{\"id\":", 6) || !text_json_string(out, id) || !text_add(out, ",\"name\":", 8) ||
        !text_json_string(out, name) || !text_add(out, ",\"is_default\":", 14)) return 0;
    const char *flag = dream_audio_device_default(index) ? "true}" : "false}";
    return text_add(out, flag, strlen(flag));
}

static char *device_json(void) {
    text out = {0};
    if (!text_add(&out, "[", 1)) {
        free(out.data);
        return NULL;
    }
    int count = dream_audio_device_count();
    int first = 1;
    for (int index = 0; index < count; index++) {
        size_t mark = out.len;
        if (!append_device(&out, index, first)) {
            free(out.data);
            return NULL;
        }
        if (out.len != mark) first = 0;
    }
    dream_audio_context_idle();
    if (!text_add(&out, "]", 1)) {
        free(out.data);
        return NULL;
    }
    return out.data;
}

static int device_index(const char *id) {
    int count = dream_audio_device_count();
    for (int index = 0; index < count; index++) {
        const char *name = dream_audio_device_name(index);
        if (!name) continue;
        char built[1024];
        snprintf(built, sizeof(built), "native:%d:%s", index, name);
        if (strcmp(built, id) == 0) return index;
    }
    return -2;
}

int32_t audio_output_create(const float *samples, int32_t count, int32_t rate, int32_t channels) {
    lock_players();
    int32_t handle = player_insert(dream_audio_output_create(samples, count, rate, channels));
    unlock_players();
    return handle;
}

int32_t audio_output_stream(int32_t rate, int32_t channels, int32_t frames) {
    lock_players();
    int32_t handle = player_insert(dream_audio_output_stream(rate, channels, frames));
    unlock_players();
    return handle;
}

int32_t audio_output_write(int32_t handle, const float *samples, int32_t count) {
    lock_players();
    void *player = player_get(handle);
    int32_t written = player ? dream_audio_output_write(player, samples, count) : 0;
    unlock_players();
    return written;
}

int32_t audio_output_start(int32_t handle) {
    lock_players();
    void *player = player_get(handle);
    int32_t started = player ? dream_audio_output_control(player, 1, 0.0) : 0;
    unlock_players();
    return started;
}

int32_t audio_output_control(int32_t handle, int32_t command, double value) {
    if (!isfinite(value) || handle <= 0) return 0;
    lock_players();
    void *player = player_get(handle);
    int32_t result = player ? dream_audio_output_control(player, command, value) : 0;
    unlock_players();
    return result;
}

double audio_output_query(int32_t handle, int32_t field) {
    if (handle <= 0) return 0.0;
    lock_players();
    void *player = player_get(handle);
    double result = player ? dream_audio_output_query(player, field) : 0.0;
    unlock_players();
    return result;
}

void audio_output_close(int32_t handle) {
    if (handle <= 0) return;
    lock_players();
    player_remove(handle);
    unlock_players();
}

void *audio_output_devices(void) {
    lock_players();
    char *json = device_json();
    unlock_players();
    if (!json) return dream_string_from_utf8("[]", 2);
    void *owned = dream_string_from_utf8(json, strlen(json));
    free(json);
    return owned;
}

void *audio_output_request(void) {
    return dream_string_from_utf8("[]", 2);
}

int32_t audio_output_sink(int32_t handle, const char *id) {
    if (handle <= 0) return 0;
    lock_players();
    int index = -1;
    if (id && id[0]) {
        index = device_index(id);
        if (index < 0) {
            dream_audio_context_idle();
            unlock_players();
            return 0;
        }
    }
    void *player = player_get(handle);
    int32_t result = player ? dream_audio_output_sink(player, index) : 0;
    dream_audio_context_idle();
    unlock_players();
    return result;
}

void *audio_output_capture(int32_t handle, int32_t frames) {
    if (handle <= 0 || frames < 0 || frames > 32768) return dream_scalar_array_new((int32_t)sizeof(float), 0);
    float *scratch = calloc((size_t)(5 + frames * 2), sizeof(float));
    if (!scratch) return dream_scalar_array_new((int32_t)sizeof(float), 0);
    lock_players();
    void *player = player_get(handle);
    int count = player ? dream_audio_output_capture(player, frames, scratch) : 0;
    unlock_players();
    if (count < 0) count = 0;
    void *array = dream_scalar_array_new((int32_t)sizeof(float), count);
    float *dest = dream_array_data(array);
    if (dest && count > 0) memcpy(dest, scratch, (size_t)count * sizeof(float));
    free(scratch);
    return array;
}

int32_t audio_media_create(const char *src) {
    (void)src;
    return 0;
}

int32_t audio_media_seek(int32_t handle, double time) {
    (void)handle;
    (void)time;
    return -1;
}
