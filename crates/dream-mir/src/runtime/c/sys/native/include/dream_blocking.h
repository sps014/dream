#ifndef DREAM_BLOCKING_H
#define DREAM_BLOCKING_H

/* Accepted jobs own their contexts until run returns. Rejected submissions leave ownership
 * with the caller. Admission never blocks a scheduler thread; no guest pointers are retained
 * automatically. Jobs must use the foreign-completion API to resume their owning scheduler. */
typedef void (*dream_blocking_work)(void *context);
int dream_blocking_submit(dream_blocking_work work, void *context);

#endif
