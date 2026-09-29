#pragma once
#include <stddef.h>
#include <stdint.h>

typedef struct ticker ticker;

/* Called once per tick with the registered user data first; the result is added to the total. */
typedef int (*ticker_fn)(void* user, int tick);

ticker* ticker_new(const char* name);
void ticker_free(ticker* t);
const char* ticker_name(const ticker* t);
void ticker_on_tick(ticker* t, ticker_fn fn, void* user);
int ticker_run(ticker* t, int ticks);
/* NULL past the last tick run so far. */
const char* ticker_label(ticker* t, int tick);
/* FNV-1a, folded into a non-negative int. */
int checksum(const uint8_t* data, size_t len);
