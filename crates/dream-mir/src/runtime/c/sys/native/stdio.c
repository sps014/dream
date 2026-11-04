#include "dream_host_support.h"
#include "dream_platform_internal.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef _WIN32
#include <conio.h>
#include <io.h>
#include <windows.h>
#else
#include <sys/stat.h>
#include <unistd.h>
#endif

/* A pipe has a reader waiting on the other end. A regular file does not, so it can fill an 8 KiB
 * buffer. `exit` and panic flush either way. */
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
        setvbuf(stdout, NULL, line ? _IOLBF : _IOFBF, line ? 0 : 8192);
        __atomic_store_n(&ready, 1, __ATOMIC_RELEASE);
    }
#ifdef _WIN32
    _unlock_file(stdout);
#else
    funlockfile(stdout);
#endif
}

static void write_string(FILE *stream, dream_ptr s) {
    char text[1024];
    const uint16_t *units = dream_str_units(s);
    int32_t len = dream_str_len(s);
    int32_t position = 0;
    if (stream == stdout) {
        dream_configure_stdout();
    }
#ifdef _WIN32
    _lock_file(stream);
#else
    flockfile(stream);
#endif
    // One logical write remains indivisible even when conversion requires several chunks.
    while (position < len) {
        size_t written;
        position = dream_utf8_chunk(units, len, text, sizeof(text), position, &written, 1);
        fwrite(text, 1, written, stream);
    }
    /* stderr stays unbuffered from the caller's point of view. stdout follows the policy set
     * above, so a redirected program is not one syscall per print. */
    if (stream != stdout) {
        fflush(stream);
    }
#ifdef _WIN32
    _unlock_file(stream);
#else
    funlockfile(stream);
#endif
}

void print_string(dream_ptr s) { write_string(stdout, s); }
void print_err_string(dream_ptr s) { write_string(stderr, s); }

void print_err_char(int32_t c) {
    fputc(c == 10 ? '\n' : (int)c, stderr);
    fflush(stderr);
}

dream_ptr consoleReadLine(void) {
    char buf[4096];
    size_t n;
    if (!fgets(buf, sizeof(buf), stdin)) {
        return dream_str_from_utf8("");
    }
    n = strlen(buf);
    if (n > 0 && buf[n - 1] == '\n') {
        buf[n - 1] = 0;
        n--;
        if (n > 0 && buf[n - 1] == '\r') {
            buf[n - 1] = 0;
        }
    }
    return dream_str_from_utf8(buf);
}

int32_t consoleReadKey(void) {
#ifdef _WIN32
    return _getch();
#else
    return fgetc(stdin);
#endif
}

void consoleExit(int32_t code) { exit(code); }
