#ifndef DREAM_PROCESS_H
#define DREAM_PROCESS_H

#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#include "dream_host_windows.h"
typedef HANDLE dream_pipe;
#define DREAM_PIPE_NONE NULL
#else
typedef int dream_pipe;
#define DREAM_PIPE_NONE (-1)
#endif

/* Parent pipe ends: [0] writes the child's stdin, [1] and [2] read its stdout and stderr.
 * POSIX ends are close-on-exec and nonblocking; Windows ends are not inheritable. */
typedef struct {
#ifdef _WIN32
    HANDLE process;
#else
    int pid;
#endif
    dream_pipe pipes[3];
} DreamSpawned;

enum { DREAM_SPAWN_OK = 0, DREAM_SPAWN_INVALID = 1, DREAM_SPAWN_FAILED = 2 };

/* `env` alternates keys and values. Fills `message` (NUL-terminated) on failure. */
int dream_process_spawn(const char *command, char *const *args, int32_t argc, const char *cwd,
                        char *const *env, int32_t envc, int clear_env, DreamSpawned *out,
                        char *message, size_t message_size);

#ifndef _WIN32
/* Close-on-exec pipe. Serialized with spawning where the flag cannot be set atomically, so a
 * concurrent fork never inherits the descriptors. */
int dream_process_pipe(int fds[2]);
#endif

#endif
