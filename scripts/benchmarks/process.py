"""Process accounting shared by the benchmark commands."""
import os
from pathlib import Path
import subprocess


def dream_name() -> str:
    return "dream.exe" if os.name == "nt" else "dream"


def with_exe(path: Path) -> Path:
    """Use an existing path, or the Windows `.exe` form when that file is present."""
    path = Path(path)
    if path.is_file() or os.name != "nt" or path.suffix.lower() == ".exe":
        return path
    exe = path.with_name(path.name + ".exe")
    return exe if exe.is_file() else path


def peak_working_set(proc: subprocess.Popen) -> int:
    """Peak working set in bytes for a finished Windows process.

    The kernel keeps this high-water mark on the process handle after exit, which is the
    same quantity `wait4` reports as `ru_maxrss` for the direct child.
    """
    import ctypes
    from ctypes import wintypes

    class ProcessMemoryCounters(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.K32GetProcessMemoryInfo.argtypes = [
        wintypes.HANDLE, ctypes.POINTER(ProcessMemoryCounters), wintypes.DWORD,
    ]
    kernel.K32GetProcessMemoryInfo.restype = wintypes.BOOL
    info = ProcessMemoryCounters()
    info.cb = ctypes.sizeof(info)
    handle = getattr(proc, "_handle", None)
    if handle is None or not kernel.K32GetProcessMemoryInfo(int(handle), ctypes.byref(info), info.cb):
        raise RuntimeError(f"peak working set unavailable for pid {proc.pid}")
    return int(info.PeakWorkingSetSize)
