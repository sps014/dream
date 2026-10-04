/* External definitions of every `DREAM_ALWAYS_INLINE` helper in `dream_core.h`, for the
 * LLVM backend: it calls these by name, and after `llvm-link` + internalize LLVM inlines them
 * the way clang would inline them from the header. No `used`: that would pin unreferenced helpers
 * through internalize. External definitions are always emitted, so the signature table still
 * sees every helper. */
#define DREAM_ALWAYS_INLINE __attribute__((always_inline))
#include "dream_core.h"
