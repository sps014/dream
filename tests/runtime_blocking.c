#include "dream_blocking.h"
#include "dream_thread.h"
#include <assert.h>
#include <stdio.h>

static dream_mutex mu = DREAM_MUTEX_INIT;
static dream_cond changed = DREAM_COND_INIT;
static unsigned entered, active, peak, completed;
static int released;
static unsigned visits[132];

static void work(void *context) {
    unsigned *visit = context;
    dream_mutex_lock(&mu);
    ++entered;
    ++active;
    if (active > peak) { peak = active; }
    dream_cond_broadcast(&changed);
    while (!released) { dream_cond_wait(&changed, &mu); }
    ++*visit;
    --active;
    ++completed;
    dream_cond_broadcast(&changed);
    dream_mutex_unlock(&mu);
}

int main(void) {
    assert(!dream_blocking_submit(NULL, NULL));
    for (unsigned i = 0; i < 4; ++i) { assert(dream_blocking_submit(work, &visits[i])); }
    dream_mutex_lock(&mu);
    while (entered != 4) { dream_cond_wait(&changed, &mu); }
    dream_mutex_unlock(&mu);
    for (unsigned i = 4; i < 132; ++i) { assert(dream_blocking_submit(work, &visits[i])); }
    unsigned rejected = 0;
    assert(!dream_blocking_submit(work, &rejected));
    dream_mutex_lock(&mu);
    assert(active == 4 && !completed && !rejected);
    released = 1;
    dream_cond_broadcast(&changed);
    while (completed != 132) { dream_cond_wait(&changed, &mu); }
    assert(peak == 4 && active == 0 && !rejected);
    for (unsigned i = 0; i < 132; ++i) { assert(visits[i] == 1); }
    dream_mutex_unlock(&mu);
    puts("bounded blocking executor passed");
    return 0;
}
