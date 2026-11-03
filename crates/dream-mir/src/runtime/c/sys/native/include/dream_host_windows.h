#ifndef DREAM_HOST_WINDOWS_H
#define DREAM_HOST_WINDOWS_H

#ifdef _WIN32
/* Full windows.h pulls in COM headers that autolink uuid.lib, which a cross sysroot may lack. */
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#endif

#endif
