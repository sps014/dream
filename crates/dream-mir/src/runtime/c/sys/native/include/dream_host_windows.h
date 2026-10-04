#ifndef DREAM_HOST_WINDOWS_H
#define DREAM_HOST_WINDOWS_H

#ifdef _WIN32
/* Full windows.h pulls in COM headers whose uuid.lib pragma the Zig linker can't satisfy. */
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

#endif
