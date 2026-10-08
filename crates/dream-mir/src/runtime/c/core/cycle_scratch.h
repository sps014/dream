#ifndef DREAM_CYCLE_SCRATCH_H
#define DREAM_CYCLE_SCRATCH_H

/* Scratch records are pooled separately from nodes, whose queued identities must stay pinned. */
/* Queue entries recycle through a gate-guarded free list: teardown of a tracked graph queues
 * one entry per object. */
static Candidate *free_candidates;

static Candidate *take_candidate(void) {
    Candidate *candidate = free_candidates;
    if (candidate) {
        free_candidates = candidate->next;
        *candidate = (Candidate){0};
        return candidate;
    }
    candidate = dream_raw_calloc(1, sizeof(*candidate));
    if (!candidate) { DREAM_PANIC_LITERAL(u"panic: out of memory queuing cycle candidate"); }
    return candidate;
}

static void give_candidate(Candidate *candidate) {
    candidate->next = free_candidates;
    free_candidates = candidate;
}


/* A trial entry hangs off its collector node for the duration of one gated trial, so membership
 * is a pointer test; entries recycle through a gate-guarded pool. */
typedef struct Trial {
    dream_ptr ptr;
    CycleNode *node;
    int64_t residual;
    int live;
    struct Trial *next;
    struct Trial *member;
} Trial;

typedef struct {
    Trial *members;
    Trial *last_member;
    Trial *head;
    Trial *tail;
    int mode;
} TrialGraph;

static Trial *free_trials;

static Trial *take_trial(void) {
    Trial *entry = free_trials;
    if (entry) {
        free_trials = entry->member;
        *entry = (Trial){0};
        return entry;
    }
    entry = dream_raw_calloc(1, sizeof(*entry));
    if (!entry) { DREAM_PANIC_LITERAL(u"panic: out of memory constructing cycle trial"); }
    return entry;
}

static void trial_edge(dream_ptr ptr, void *arg) {
    TrialGraph *graph = arg;
    dream_count(DREAM_COUNT_EDGES, 1);
    CycleNode *node = find_node(ptr);
    if (!node || node->dying) { return; }
    Trial *entry = node->trial;
    if (graph->mode == 0 && !entry) {
        entry = take_trial();
        entry->ptr = ptr;
        entry->node = node;
        entry->residual = dream_rc_count(ptr);
        node->trial = entry;
        if (graph->last_member) { graph->last_member->member = entry; } else { graph->members = entry; }
        graph->last_member = entry;
        if (graph->tail) { graph->tail->next = entry; } else { graph->head = entry; }
        graph->tail = entry;
    } else if (graph->mode == 1 && entry) {
        --entry->residual;
    } else if (graph->mode == 2 && entry && !entry->live) {
        entry->live = 1;
        entry->next = NULL;
        if (graph->tail) { graph->tail->next = entry; } else { graph->head = entry; }
        graph->tail = entry;
    }
}

/* Bottom-up merge sort by allocation identity: destruction order is part of the observable
 * cleanup trace, and the freestanding wasm runtime has no qsort. */
static Trial *sort_by_allocation(Trial *list, size_t count) {
    for (size_t width = 1; width < count; width *= 2) {
        Trial *result = NULL, **tail = &result, *rest = list;
        while (rest) {
            Trial *left = rest, *right = rest;
            size_t left_n = 0, right_n = 0;
            while (right && left_n < width) { right = right->next; ++left_n; }
            rest = right;
            while (rest && right_n < width) { rest = rest->next; ++right_n; }
            while (left_n || right_n) {
                int take_left = left_n && (!right_n || left->node->sequence < right->node->sequence);
                Trial *pick = take_left ? left : right;
                if (take_left) { left = left->next; --left_n; } else { right = right->next; --right_n; }
                *tail = pick;
                tail = &pick->next;
            }
        }
        *tail = NULL;
        list = result;
    }
    return list;
}

#endif
