/* Adapted from musl src/string/wcsstr.c (Two-Way search).
 * https://git.musl-libc.org/cgit/musl/tree/src/string/wcsstr.c
 * Explicit UTF-16 view lengths replace terminator scanning, including embedded zero units.
 * Copyright © 2005-2020 Rich Felker, et al.
 * Permission is hereby granted, free of charge, to any person obtaining
 * a copy of this software and associated documentation files (the
 * "Software"), to deal in the Software without restriction, including
 * without limitation the rights to use, copy, modify, merge, publish,
 * distribute, sublicense, and/or sell copies of the Software, and to
 * permit persons to whom the Software is furnished to do so, subject to
 * the following conditions:
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
 * MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
 * IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
 * CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
 * TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
 * SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
 */
#ifndef DREAM_STRING_SEARCH_H
#define DREAM_STRING_SEARCH_H

/* The size_t -1 suffix sentinel wraps only in index arithmetic, never in a pointer.
 * Known lengths bound every comparison; no preprocessing storage or guest allocation is needed. */
static int32_t find_long_needle(const uint16_t *h, size_t hn, const uint16_t *n, size_t l) {
    size_t ip, jp, k, p, ms, p0, mem, mem0, pos = 0;
    ip = -1; jp = 0; k = p = 1;
    while (jp+k<l) {
        if (n[ip+k] == n[jp+k]) {
            if (k == p) {
                jp += p;
                k = 1;
            } else k++;
        } else if (n[ip+k] > n[jp+k]) {
            jp += k;
            k = 1;
            p = jp - ip;
        } else {
            ip = jp++;
            k = p = 1;
        }
    }
    ms = ip;
    p0 = p;

    ip = -1; jp = 0; k = p = 1;
    while (jp+k<l) {
        if (n[ip+k] == n[jp+k]) {
            if (k == p) {
                jp += p;
                k = 1;
            } else k++;
        } else if (n[ip+k] < n[jp+k]) {
            jp += k;
            k = 1;
            p = jp - ip;
        } else {
            ip = jp++;
            k = p = 1;
        }
    }
    if (ip+1 > ms+1) ms = ip;
    else p = p0;

    if (!dream_units_equal(n, n+p, (ms+1) * sizeof(*n))) {
        mem0 = 0;
        p = (ms > l-ms-1 ? ms : l-ms-1) + 1;
    } else mem0 = l-p;
    mem = 0;

    while (pos <= hn - l) {
        for (k = ms+1 > mem ? ms+1 : mem; k < l && n[k] == h[pos+k]; k++);
        if (k < l) {
            pos += k-ms;
            mem = 0;
            continue;
        }
        for (k = ms+1; k > mem && n[k-1] == h[pos+k-1]; k--);
        if (k <= mem) { return (int32_t)pos; }
        pos += p;
        mem = mem0;
    }
    return -1;
}
#endif
