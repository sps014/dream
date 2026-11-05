#include "dream_platform_internal.h"

#include <stdio.h>
#ifdef _WIN32
#include <io.h>
#include <windows.h>
#else
#include <sys/stat.h>
#include <unistd.h>
#endif

/* A pipe has a reader waiting on the other end. A regular file does not, so it can fill an 8 KiB
 * buffer. `exit` and panic flush either way. Its own unit so a test can link `stdio.c` or
 * `platform.c` without the other. */
static int stdout_is_pipe(void) {
#ifdef _WIN32
    return GetFileType(GetStdHandle(STD_OUTPUT_HANDLE)) == FILE_TYPE_PIPE;
#else
    struct stat st;
    return fstat(fileno(stdout), &st) == 0 && S_ISFIFO(st.st_mode);
#endif
}

void dream_configure_stdout(void) {
    static int ready;
    if (__atomic_load_n(&ready, __ATOMIC_ACQUIRE)) {
        return;
    }
#ifdef _WIN32
    int tty = _isatty(_fileno(stdout));
    _lock_file(stdout);
#else
    int tty = isatty(fileno(stdout));
    flockfile(stdout);
#endif
    if (!__atomic_load_n(&ready, __ATOMIC_RELAXED)) {
        int line = tty || stdout_is_pipe();
#ifdef _WIN32
        /* UCRT rejects a zero size and fast-fails the process, and its _IOLBF is full
         * buffering. A console or pipe has to stay unbuffered so a reader is not stuck
         * behind 8 KiB; a regular file can fill that buffer. */
        setvbuf(stdout, NULL, line ? _IONBF : _IOFBF, line ? 2 : 8192);
#else
        setvbuf(stdout, NULL, line ? _IOLBF : _IOFBF, line ? 0 : 8192);
#endif
        __atomic_store_n(&ready, 1, __ATOMIC_RELEASE);
    }
#ifdef _WIN32
    _unlock_file(stdout);
#else
    funlockfile(stdout);
#endif
}
