#include "dream_host_support.h"
#include "dream_platform_internal.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef _WIN32
#include <conio.h>
#endif

static void write_string(FILE *stream, dream_ptr s) {
    char text[1024];
    const uint16_t *units = dream_str_units(s);
    int32_t len = dream_str_len(s);
    int32_t position = 0;
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
    fflush(stream);
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
