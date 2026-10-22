#include "network.h"
#include <ctype.h>

/* Standalone WASI ships no UTS #46 tables, so internationalized domains are rejected and
 * ASCII Punycode labels are accepted as written. */
dream_ptr netDomainToAscii(dream_ptr domain) {
    char *text = dream_str_utf8(domain);
    if (!text) { return dream_str_from_utf8(""); }
    for (char *c = text; *c; c++) {
        if ((unsigned char)*c >= 0x80) { free(text); return dream_str_from_utf8(""); }
        *c = (char)tolower((unsigned char)*c);
    }
    dream_ptr result = dream_str_from_utf8(text);
    free(text);
    return result;
}
