#include "include/dream_rt_native.h"

#include <pthread.h>
#include <stdlib.h>
#include <string.h>

extern dream_ptr dream_worker_invoke(int32_t fn, dream_ptr env, dream_ptr arg);
static dream_ptr worker_recv_blocking(int32_t id);

typedef struct Job {
    int32_t fn;
    dream_ptr env;
    dream_ptr msg;
    struct Job *next;
} Job;

typedef struct Worker {
    int32_t id;
    int32_t fn;
    dream_ptr env;
    pthread_t th;
    pthread_mutex_t mu;
    pthread_cond_t cv;
    Job *head;
    Job *tail;
    dream_ptr reply;
    int has_reply;
    int dead;
} Worker;

#define MAX_WORKERS 64
static Worker *workers[MAX_WORKERS];
static pthread_mutex_t reg_mu = PTHREAD_MUTEX_INITIALIZER;
static int32_t next_id = 1;

static void *worker_main(void *arg) {
    Worker *w = (Worker *)arg;
    for (;;) {
        pthread_mutex_lock(&w->mu);
        while (!w->head && !w->dead) {
            pthread_cond_wait(&w->cv, &w->mu);
        }
        if (w->dead && !w->head) {
            pthread_mutex_unlock(&w->mu);
            break;
        }
        Job *j = w->head;
        w->head = j->next;
        if (!w->head) {
            w->tail = NULL;
        }
        pthread_mutex_unlock(&w->mu);
        dream_ptr r = dream_worker_invoke(j->fn, j->env, j->msg);
        dream_release(j->msg);
        free(j);
        pthread_mutex_lock(&w->mu);
        w->reply = r;
        w->has_reply = 1;
        pthread_cond_signal(&w->cv);
        pthread_mutex_unlock(&w->mu);
    }
    return NULL;
}

static Worker *find_worker(int32_t id) {
    int i;
    for (i = 0; i < MAX_WORKERS; i++) {
        if (workers[i] && workers[i]->id == id) {
            return workers[i];
        }
    }
    return NULL;
}

int32_t workerSpawn(int32_t fn, int64_t env) {
    Worker *w = (Worker *)calloc(1, sizeof(Worker));
    int i;
    w->fn = fn;
    w->env = (dream_ptr)(uintptr_t)env;
    pthread_mutex_init(&w->mu, NULL);
    pthread_cond_init(&w->cv, NULL);
    pthread_mutex_lock(&reg_mu);
    w->id = next_id++;
    for (i = 0; i < MAX_WORKERS; i++) {
        if (!workers[i]) {
            workers[i] = w;
            break;
        }
    }
    pthread_mutex_unlock(&reg_mu);
    pthread_create(&w->th, NULL, worker_main, w);
    return w->id;
}

int32_t workerPoolSpawn(void) { return workerSpawn(0, 0); }

void workerPost(int32_t id, dream_ptr msg) {
    Worker *w;
    Job *j;
    pthread_mutex_lock(&reg_mu);
    w = find_worker(id);
    pthread_mutex_unlock(&reg_mu);
    if (!w) {
        return;
    }
    j = (Job *)calloc(1, sizeof(Job));
    j->fn = w->fn;
    j->env = w->env;
    j->msg = msg;
    dream_retain(msg);
    pthread_mutex_lock(&w->mu);
    if (w->tail) {
        w->tail->next = j;
    } else {
        w->head = j;
    }
    w->tail = j;
    pthread_cond_signal(&w->cv);
    pthread_mutex_unlock(&w->mu);
}

dream_ptr workerPoolDispatch(int32_t id, int32_t fn, int64_t env, dream_ptr msg) {
    Worker *w;
    Job *j;
    pthread_mutex_lock(&reg_mu);
    w = find_worker(id);
    pthread_mutex_unlock(&reg_mu);
    if (!w) {
        return 0;
    }
    j = (Job *)calloc(1, sizeof(Job));
    j->fn = fn;
    j->env = (dream_ptr)(uintptr_t)env;
    j->msg = msg;
    dream_retain(msg);
    pthread_mutex_lock(&w->mu);
    if (w->tail) {
        w->tail->next = j;
    } else {
        w->head = j;
    }
    w->tail = j;
    pthread_cond_signal(&w->cv);
    pthread_mutex_unlock(&w->mu);
    return worker_recv_blocking(id);
}

static dream_ptr worker_recv_blocking(int32_t id) {
    Worker *w;
    dream_ptr r;
    pthread_mutex_lock(&reg_mu);
    w = find_worker(id);
    pthread_mutex_unlock(&reg_mu);
    if (!w) {
        return 0;
    }
    pthread_mutex_lock(&w->mu);
    while (!w->has_reply && !w->dead) {
        pthread_cond_wait(&w->cv, &w->mu);
    }
    r = w->reply;
    w->reply = 0;
    w->has_reply = 0;
    pthread_mutex_unlock(&w->mu);
    return r;
}

dream_ptr workerRecv(int32_t id) { return worker_recv_blocking(id); }

void workerTerminate(int32_t id) {
    Worker *w;
    pthread_mutex_lock(&reg_mu);
    w = find_worker(id);
    pthread_mutex_unlock(&reg_mu);
    if (!w) {
        return;
    }
    pthread_mutex_lock(&w->mu);
    w->dead = 1;
    pthread_cond_signal(&w->cv);
    pthread_mutex_unlock(&w->mu);
}
