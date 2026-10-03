#include "ticker.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct ticker {
    char* name;
    ticker_fn fn;
    void* user;
    int ticks;
    char label[64];
};

ticker* ticker_new(const char* name) {
    ticker* t = calloc(1, sizeof *t);
    t->name = strdup(name);
    return t;
}

void ticker_free(ticker* t) {
    printf("[c] ticker_free(%s)\n", t->name);
    fflush(stdout);
    free(t->name);
    free(t);
}

const char* ticker_name(const ticker* t) { return t->name; }

void ticker_on_tick(ticker* t, ticker_fn fn, void* user) {
    t->fn = fn;
    t->user = user;
}

int ticker_run(ticker* t, int ticks) {
    int total = 0;
    for (int i = 0; i < ticks; i++) {
        int tick = t->ticks++;
        if (t->fn) {
            total += t->fn(t->user, tick);
        }
    }
    return total;
}

const char* ticker_label(ticker* t, int tick) {
    if (tick < 0 || tick >= t->ticks) {
        return NULL;
    }
    snprintf(t->label, sizeof t->label, "%s#%d", t->name, tick);
    return t->label;
}

int checksum(const uint8_t* data, size_t len) {
    uint32_t h = 2166136261u;
    for (size_t i = 0; i < len; i++) {
        h ^= data[i];
        h *= 16777619u;
    }
    return (int)(h % 1000003u);
}
