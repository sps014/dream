/* External definitions of every `DREAM_ALWAYS_INLINE` helper in `dream_core.h`, for the
 * LLVM backend: it calls these by name, and after `llvm-link` + internalize LLVM inlines them
 * the way clang would inline them from the header. No `used`: that would pin unreferenced helpers
 * through internalize. External definitions are always emitted, so the signature table still
 * sees every helper. */
/* Size builds let the whole-program inliner weigh duplicated ownership and validation paths. */
#if defined(__OPTIMIZE_SIZE__)
#define DREAM_ALWAYS_INLINE
#else
#define DREAM_ALWAYS_INLINE __attribute__((always_inline))
#endif
#include "dream_core.h"
