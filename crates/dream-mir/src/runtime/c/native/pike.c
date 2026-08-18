#include "include/dream_rt_native.h"

enum {
    OP_CHARCLASS = 0,
    OP_CAPTURE = 1,
    OP_EMPTY = 2,
    OP_ALT = 3,
    OP_MATCH = 4,
    OP_NOP = 5,
    OP_LOOK = 6,
    OP_OTHER = 7,
};

/* Threaded dispatch over flattened Pike opcodes. Consuming ops (CharClass/Match/Look/other)
 * are pushed to `out_pcs`; epsilon ops walk via computed goto. */

int dream_pike_add_to_threadq(const int32_t *opcodes, const int32_t *outs, const int32_t *out1s,
                              const int32_t *cap_ids, int32_t ninst, int32_t start_pc, int32_t pos,
                              int32_t *on_list, int32_t gen, int32_t *out_pcs, int32_t *out_n) {
    int32_t stack[256];
    int32_t sp = 0;
    int nout = *out_n;
    (void)cap_ids;
    (void)pos;
    if (start_pc < 0 || start_pc >= ninst) {
        return 0;
    }
    stack[sp++] = start_pc;

#if defined(__GNUC__)
    static void *const table[] = {
        &&op_charclass, &&op_capture, &&op_empty, &&op_alt,
        &&op_match,     &&op_nop,     &&op_look,  &&op_other,
    };
#endif

    while (sp > 0) {
        int32_t pc = stack[--sp];
        int32_t op;
        if (pc < 0 || pc >= ninst) {
            continue;
        }
        if (on_list[pc] == gen) {
            continue;
        }
        on_list[pc] = gen;
        op = opcodes[pc];
        if ((uint32_t)op > 7u) {
            op = OP_OTHER;
        }
#if defined(__GNUC__)
        goto *table[op];
op_nop:
        stack[sp++] = outs[pc];
        continue;
op_alt:
        stack[sp++] = out1s[pc];
        stack[sp++] = outs[pc];
        continue;
op_capture:
        stack[sp++] = outs[pc];
        continue;
op_empty:
        stack[sp++] = outs[pc];
        continue;
op_look:
        out_pcs[nout++] = pc;
        continue;
op_charclass:
op_match:
op_other:
        out_pcs[nout++] = pc;
        continue;
#else
        switch (op) {
        case OP_NOP:
            stack[sp++] = outs[pc];
            break;
        case OP_ALT:
            stack[sp++] = out1s[pc];
            stack[sp++] = outs[pc];
            break;
        case OP_CAPTURE:
        case OP_EMPTY:
            stack[sp++] = outs[pc];
            break;
        default:
            out_pcs[nout++] = pc;
            break;
        }
#endif
    }
    *out_n = nout;
    return 1;
}
