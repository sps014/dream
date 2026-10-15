#include "dream_core.h"
#include <stdlib.h>
#ifndef DREAM_WASM32
#include "dream_thread.h"
#endif

typedef struct SharedChannel {
#ifdef DREAM_WASM32
    int32_t mutex;
#else
    dream_mutex mutex;
#endif
    dream_ptr *items;
    size_t capacity, head, count;
    int32_t bounded, senders, receivers, send_closed, receive_closed;
    uintptr_t readable, writable;
} SharedChannel;
typedef struct ChannelEndpoint {
    SharedChannel *channel;
    int32_t sending, closed;
} ChannelEndpoint;
#ifdef DREAM_WASM32
static void acquire(SharedChannel *s) { dream_lock_acquire((dream_ptr)(uintptr_t)&s->mutex); }
static void unlock(SharedChannel *s) { dream_lock_release((dream_ptr)(uintptr_t)&s->mutex); }
#else
static void acquire(SharedChannel *s) { dream_mutex_lock(&s->mutex); }
static void unlock(SharedChannel *s) { dream_mutex_unlock(&s->mutex); }
#endif
static void notify(uintptr_t *signal) {
    if (!*signal) { return; }
    dream_cancellation_cancel(*signal);
    dream_cancellation_release(*signal);
    *signal = 0;
}

uintptr_t dream_channel_new(int32_t capacity) {
    if (capacity < 0) { return 0; }
    SharedChannel *s = calloc(1, sizeof(*s));
    if (!s) { DREAM_PANIC_LITERAL(u"out of memory creating channel"); }
#ifndef DREAM_WASM32
    dream_mutex_init(&s->mutex);
#endif
    s->bounded = capacity > 0;
    s->capacity = capacity > 0 ? (size_t)capacity : 8;
    s->items = calloc(s->capacity, sizeof(dream_ptr));
    if (!s->items) { DREAM_PANIC_LITERAL(u"out of memory creating channel queue"); }
    return (uintptr_t)s;
}

void dream_channel_free(uintptr_t handle) {
    SharedChannel *s = (SharedChannel *)handle;
    if (!s) { return; }
    for (size_t i = 0; i < s->count; ++i) { dream_release(s->items[(s->head + i) % s->capacity]); }
    notify(&s->readable);
    notify(&s->writable);
#ifndef DREAM_WASM32
    dream_mutex_destroy(&s->mutex);
#endif
    free(s->items);
    free(s);
}

uintptr_t dream_channel_endpoint(uintptr_t handle, int32_t sending) {
    SharedChannel *s = (SharedChannel *)handle;
    ChannelEndpoint *endpoint = calloc(1, sizeof(*endpoint));
    if (!endpoint) { DREAM_PANIC_LITERAL(u"out of memory creating channel endpoint"); }
    endpoint->channel = s;
    endpoint->sending = sending;
    acquire(s);
    if (sending) { ++s->senders; } else { ++s->receivers; }
    unlock(s);
    return (uintptr_t)endpoint;
}

void dream_channel_close(uintptr_t handle) {
    ChannelEndpoint *e = (ChannelEndpoint *)handle;
    SharedChannel *s = e->channel;
    dream_ptr *discard = NULL;
    size_t discarded_count = 0, discarded_head = 0, discarded_capacity = 0;
    acquire(s);
    if (!e->closed) {
        e->closed = 1;
        if (e->sending) {
            if (--s->senders == 0) { s->send_closed = 1; }
        } else if (--s->receivers == 0) {
            s->receive_closed = 1;
            discard = s->items;
            discarded_count = s->count;
            discarded_head = s->head;
            discarded_capacity = s->capacity;
            s->items = NULL;
            s->count = 0;
            s->head = 0;
        }
        notify(&s->readable);
        notify(&s->writable);
    }
    unlock(s);
    // Payload destructors may close endpoints on this same channel.
    for (size_t i = 0; i < discarded_count; ++i) {
        dream_release(discard[(discarded_head + i) % discarded_capacity]);
    }
    free(discard);
}

void dream_channel_endpoint_free(uintptr_t handle) {
    dream_channel_close(handle);
    free((void *)handle);
}

/* 0 = accepted, 1 = full, 2 = closed. The array carrier owns the typed payload. */
int32_t dream_channel_send(uintptr_t handle, dream_ptr value) {
    ChannelEndpoint *e = (ChannelEndpoint *)handle;
    SharedChannel *s = e->channel;
    acquire(s);
    if (e->closed || s->send_closed || s->receive_closed) { unlock(s); return 2; }
    if (s->count == s->capacity) {
        if (s->bounded) { unlock(s); return 1; }
        if (s->capacity > SIZE_MAX / 2 / sizeof(dream_ptr)) {
            unlock(s); DREAM_PANIC_LITERAL(u"channel capacity overflow");
        }
        size_t capacity = s->capacity * 2;
        dream_ptr *items = calloc(capacity, sizeof(dream_ptr));
        if (!items) { unlock(s); DREAM_PANIC_LITERAL(u"out of memory growing channel"); }
        for (size_t i = 0; i < s->count; ++i) { items[i] = s->items[(s->head + i) % s->capacity]; }
        free(s->items);
        s->items = items;
        s->capacity = capacity;
        s->head = 0;
    }
    dream_publish(value);
    dream_retain(value);
    s->items[(s->head + s->count) % s->capacity] = value;
    ++s->count;
    dream_cancellation_notify_one(s->readable);
    unlock(s);
    return 0;
}

dream_ptr dream_channel_receive(uintptr_t handle) {
    ChannelEndpoint *e = (ChannelEndpoint *)handle;
    SharedChannel *s = e->channel;
    acquire(s);
    if (e->closed || s->receive_closed || !s->count) {
        unlock(s);
        return dream_array_new(0, 1);
    }
    dream_ptr result = s->items[s->head];
    s->items[s->head] = 0;
    s->head = (s->head + 1) % s->capacity;
    --s->count;
    dream_cancellation_notify_one(s->writable);
    unlock(s);
    return result;
}

/* 0 = open, 1 = drained, 2 = endpoint/receiver closed. */
int32_t dream_channel_status(uintptr_t handle) {
    ChannelEndpoint *e = (ChannelEndpoint *)handle;
    SharedChannel *s = e->channel;
    acquire(s);
    int32_t result = e->closed || s->receive_closed ? 2 : s->send_closed && !s->count ? 1 : 0;
    unlock(s);
    return result;
}

/* Registration and readiness check share the queue lock, preventing lost notifications. */
uintptr_t dream_channel_wait(uintptr_t handle) {
    ChannelEndpoint *e = (ChannelEndpoint *)handle;
    SharedChannel *s = e->channel;
    acquire(s);
    uintptr_t *signal = e->sending ? &s->writable : &s->readable;
    if (!*signal) { *signal = dream_cancellation_new(); }
    uintptr_t registration = dream_cancellation_register(*signal);
    int ready = e->closed || s->receive_closed || s->send_closed;
    if (e->sending) { ready |= !s->bounded || s->count < s->capacity; }
    else { ready |= s->count > 0; }
    if (ready) { dream_cancellation_notify_registration(registration); }
    unlock(s);
    return registration;
}
