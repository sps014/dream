#include "dream_host_support.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef _WIN32
#include <conio.h>
#endif

void print_int(int32_t v) { printf("%d", v); }

void print_char(int32_t c) {
    if (c == 10) {
        fputc('\n', stdout);
    } else {
        fputc((int)c, stdout);
    }
    fflush(stdout);
}

void print_string(dream_ptr s) {
    char *u = dream_str_utf8(s);
    if (u) {
        fputs(u, stdout);
        free(u);
    }
    fflush(stdout);
}

void print_err_string(dream_ptr s) {
    char *u = dream_str_utf8(s);
    if (u) {
        fputs(u, stderr);
        free(u);
    }
    fflush(stderr);
}

void print_err_char(int32_t c) {
    fputc(c == 10 ? '\n' : (int)c, stderr);
    fflush(stderr);
}

static void print_float_shortest(float v) {
    char text[32];
    char *end;
    snprintf(text, sizeof(text), "%.6f", (double)v);
    end = text + strlen(text);
    while (end > text && end[-1] == '0') {
        *--end = 0;
    }
    if (end > text && end[-1] == '.') {
        *--end = 0;
    }
    fputs(text, stdout);
}

void print_float(float v) { print_float_shortest(v); }

void print_double(double v) { printf("%.16g", v); }

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

void consoleWriteStderr(dream_ptr text) {
    char *s = dream_str_utf8(text);
    if (s) {
        fputs(s, stderr);
        fflush(stderr);
        free(s);
    }
}

void consoleExit(int32_t code) { exit(code); }
