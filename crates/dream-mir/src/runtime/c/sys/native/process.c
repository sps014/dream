#include "dream_host_support.h"
#include "dream_thread.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#ifdef __APPLE__
#include <mach-o/dyld.h>
#include <mach/mach.h>
#endif
#ifdef _WIN32
#include "dream_host_windows.h"
#include <direct.h>
#include <psapi.h>
#else
#include <sys/resource.h>
#include <unistd.h>
#endif

int32_t processPlatform(void) { return 0; }

int32_t processOsFamily(void) {
#ifdef _WIN32
    return 1;
#else
    return 0;
#endif
}

static char *dream_captured_args;
static char dream_exe_path_buf[4096];

void dream_process_capture_args(int32_t argc, char **argv) {
    int32_t i;
    size_t total = 0;
    char *join;
    dream_thread_attach();
    if (argc > 0 && argv && argv[0]) {
#ifdef __APPLE__
        uint32_t n = sizeof(dream_exe_path_buf);
        if (_NSGetExecutablePath(dream_exe_path_buf, &n) != 0) {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#elif defined(_WIN32)
        if (!GetModuleFileNameA(NULL, dream_exe_path_buf, sizeof(dream_exe_path_buf))) {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#else
        ssize_t n = readlink("/proc/self/exe", dream_exe_path_buf, sizeof(dream_exe_path_buf) - 1);
        if (n > 0) {
            dream_exe_path_buf[n] = 0;
        } else {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#endif
        dream_exe_path_buf[sizeof(dream_exe_path_buf) - 1] = 0;
    }
    for (i = 1; i < argc; i++) {
        total += strlen(argv[i]);
        if (i + 1 < argc) {
            total += 1;
        }
    }
    free(dream_captured_args);
    dream_captured_args = (char *)malloc(total + 1);
    if (!dream_captured_args) {
        return;
    }
    dream_captured_args[0] = 0;
    for (i = 1; i < argc; i++) {
        if (i > 1) {
            strcat(dream_captured_args, "\n");
        }
        strcat(dream_captured_args, argv[i]);
    }
}

dream_ptr processArgs(void) {
    return dream_str_from_utf8(dream_captured_args ? dream_captured_args : "");
}

dream_ptr processExePath(void) {
    return dream_str_from_utf8(dream_exe_path_buf);
}

dream_ptr processCwd(void) {
    char path[4096];
#ifdef _WIN32
    if (!_getcwd(path, (int)sizeof(path))) {
#else
    if (!getcwd(path, sizeof(path))) {
#endif
        return 0;
    }
    return dream_str_from_utf8(path);
}

int32_t processSetCwd(dream_ptr path) {
    char *text = dream_str_utf8(path);
#ifdef _WIN32
    int32_t ok = text && _chdir(text) == 0;
#else
    int32_t ok = text && chdir(text) == 0;
#endif
    free(text);
    return ok;
}

int64_t processCpuTimeNanos(void) {
#ifdef _WIN32
    FILETIME create, exit_t, kernel, user;
    ULARGE_INTEGER k, u;
    if (!GetProcessTimes(GetCurrentProcess(), &create, &exit_t, &kernel, &user)) {
        return 0;
    }
    k.LowPart = kernel.dwLowDateTime;
    k.HighPart = kernel.dwHighDateTime;
    u.LowPart = user.dwLowDateTime;
    u.HighPart = user.dwHighDateTime;
    return (int64_t)((k.QuadPart + u.QuadPart) * 100ULL);
#else
    struct rusage ru;
    if (getrusage(RUSAGE_SELF, &ru) != 0) {
        return 0;
    }
    return (int64_t)ru.ru_utime.tv_sec * 1000000000LL
        + (int64_t)ru.ru_utime.tv_usec * 1000LL
        + (int64_t)ru.ru_stime.tv_sec * 1000000000LL
        + (int64_t)ru.ru_stime.tv_usec * 1000LL;
#endif
}

int64_t processMemoryBytes(void) {
#ifdef _WIN32
    PROCESS_MEMORY_COUNTERS pmc;
    pmc.cb = sizeof(pmc);
    if (!GetProcessMemoryInfo(GetCurrentProcess(), &pmc, sizeof(pmc))) {
        return 0;
    }
    return (int64_t)pmc.WorkingSetSize;
#elif defined(__APPLE__)
    task_vm_info_data_t info;
    mach_msg_type_number_t count = TASK_VM_INFO_COUNT;
    if (task_info(mach_task_self(), TASK_VM_INFO, (task_info_t)&info, &count) != KERN_SUCCESS) {
        return 0;
    }
    return (int64_t)info.phys_footprint;
#else
    FILE *f;
    long pages = 0;
    long page = sysconf(_SC_PAGESIZE);
    f = fopen("/proc/self/statm", "r");
    if (f == NULL) {
        return 0;
    }
    if (fscanf(f, "%*s %ld", &pages) != 1) {
        fclose(f);
        return 0;
    }
    fclose(f);
    if (page < 1) {
        page = 4096;
    }
    return (int64_t)pages * (int64_t)page;
#endif
}
