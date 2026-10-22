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

static uint8_t *dream_captured_args;
static int32_t dream_captured_args_size;
static char dream_exe_path_buf[4096];

#ifdef _WIN32
static char **utf8_process_args(int32_t *count) {
    typedef LPWSTR *(WINAPI *args_function)(LPCWSTR, int *);
    HMODULE shell = LoadLibraryW(L"shell32.dll");
    args_function parse = shell ? (args_function)(void *)GetProcAddress(shell, "CommandLineToArgvW") : NULL;
    int argc = 0;
    LPWSTR *wide = parse ? parse(GetCommandLineW(), &argc) : NULL;
    if (!wide || argc < 0) { DREAM_PANIC_LITERAL(u"cannot read Unicode process arguments"); }
    char **args = calloc((size_t)argc, sizeof(char *));
    if (!args) { DREAM_PANIC_LITERAL(u"out of memory reading process arguments"); }
    for (int i = 0; i < argc; ++i) {
        int size = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, NULL, 0, NULL, NULL);
        if (size <= 0) { DREAM_PANIC_LITERAL(u"invalid Unicode process argument"); }
        args[i] = malloc((size_t)size);
        if (!args[i]) { DREAM_PANIC_LITERAL(u"out of memory reading process arguments"); }
        WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide[i], -1, args[i], size, NULL, NULL);
    }
    LocalFree(wide);
    FreeLibrary(shell);
    *count = argc;
    return args;
}
#endif

void dream_process_capture_args(int32_t argc, char **argv) {
    int32_t i;
    size_t total = 0;
#ifdef _WIN32
    argv = utf8_process_args(&argc);
#endif
    if (argc > 0 && argv && argv[0]) {
#ifdef __APPLE__
        uint32_t n = sizeof(dream_exe_path_buf);
        if (_NSGetExecutablePath(dream_exe_path_buf, &n) != 0) {
            strncpy(dream_exe_path_buf, argv[0], sizeof(dream_exe_path_buf) - 1);
        }
#elif defined(_WIN32)
        wchar_t wide_path[4096];
        DWORD path_size = GetModuleFileNameW(NULL, wide_path, 4096);
        if (!path_size || path_size >= 4096 ||
            !WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide_path, -1,
                                 dream_exe_path_buf, sizeof(dream_exe_path_buf), NULL, NULL)) {
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
    total = 4;
    for (i = 1; i < argc; i++) {
        size_t size = strlen(argv[i]);
        if (size > INT32_MAX - 4 || total > (size_t)INT32_MAX - 4 - size) {
            DREAM_PANIC_LITERAL(u"process arguments exceed the guest array limit");
        }
        total += 4 + size;
    }
    free(dream_captured_args);
    dream_captured_args = malloc(total);
    if (!dream_captured_args) { DREAM_PANIC_LITERAL(u"out of memory capturing process arguments"); }
    dream_captured_args_size = (int32_t)total;
    uint8_t *cursor = dream_captured_args;
    uint32_t count = argc > 0 ? (uint32_t)argc - 1 : 0;
    for (int shift = 0; shift < 32; shift += 8) { *cursor++ = (uint8_t)(count >> shift); }
    for (i = 1; i < argc; i++) {
        uint32_t size = (uint32_t)strlen(argv[i]);
        for (int shift = 0; shift < 32; shift += 8) { *cursor++ = (uint8_t)(size >> shift); }
        memcpy(cursor, argv[i], size);
        cursor += size;
    }
#ifdef _WIN32
    for (i = 0; i < argc; ++i) { free(argv[i]); }
    free(argv);
#endif
}

dream_ptr processArgs(void) {
    int32_t size = dream_captured_args ? dream_captured_args_size : 4;
    dream_ptr bytes = dream_array_new(size, 1);
    if (dream_captured_args) { memcpy((char *)dream_array_bytes(bytes), dream_captured_args, (size_t)size); }
    return bytes;
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
