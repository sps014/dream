#include "dream_core.h"
#include "dream_platform_internal.h"

/* Panics can fire on allocation failure or with allocator locks held, so the hook's UTF-8 copy
 * lives in a fixed buffer (truncated on a code-point boundary) rather than on the heap. */
int32_t dream_utf8_chunk(const uint16_t *units, int32_t len, char *out, size_t cap, int32_t start, size_t *written) {
    size_t n = 0;
    int32_t i;
    for (i = start; i < len; i++) {
        int32_t first = i;
        uint32_t c = units[i];
        if (c >= 0xD800 && c < 0xDC00 && i + 1 < len) {
            uint32_t lo = units[i + 1];
            if (lo >= 0xDC00 && lo < 0xE000) {
                c = 0x10000 + ((c - 0xD800) << 10) + (lo - 0xDC00);
                i++;
            }
        }
        if (c >= 0xD800 && c < 0xE000) {
            c = 0xFFFD;
        }
        size_t w = c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
        if (n + w >= cap) {
            i = first;
            break;
        }
        if (w == 1) {
            out[n++] = (char)c;
        } else if (w == 2) {
            out[n++] = (char)(0xC0 | (c >> 6));
            out[n++] = (char)(0x80 | (c & 0x3F));
        } else if (w == 3) {
            out[n++] = (char)(0xE0 | (c >> 12));
            out[n++] = (char)(0x80 | ((c >> 6) & 0x3F));
            out[n++] = (char)(0x80 | (c & 0x3F));
        } else {
            out[n++] = (char)(0xF0 | (c >> 18));
            out[n++] = (char)(0x80 | ((c >> 12) & 0x3F));
            out[n++] = (char)(0x80 | ((c >> 6) & 0x3F));
            out[n++] = (char)(0x80 | (c & 0x3F));
        }
    }
    out[n] = 0;
    *written = n;
    return i;
}

void dream_write_utf16(int stream, const uint16_t *units, int32_t len) {
    char text[1024];
    int32_t position = 0;
    do {
        size_t written;
        position = dream_utf8_chunk(units, len, text, sizeof(text), position, &written);
        dream_platform_current->write(stream, text, written, DREAM_TEXT_UTF8);
    } while (position < len);
}
