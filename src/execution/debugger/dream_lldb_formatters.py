"""LLDB formatters for Dream debug sessions.

Auto-imported by `dream debug-adapter` (see src/execution/debugger/mod.rs) so string and array
values render as text instead of raw layout fields. Everything here is presentation-only.
"""

import lldb


def _deref_until(value, want):
    """Dereferences `value` until it stops being a pointer; None if `want` never appears."""
    seen = 0
    target = None
    while value.TypeIsPointerType() and seen < 4:
        value = value.Dereference()
        if value is None or not value.IsValid():
            return None
        name = (value.GetType().GetName() or "").replace("const ", "").strip()
        if name == want:
            target = value
            break
        seen += 1
    return target


def _read_units(str_value):
    """Reads a dream_Str struct's UTF-16 units; returns a python str or None."""
    length = str_value.GetChildMemberWithName("len").GetValueAsSigned()
    if length <= 0:
        return ""
    err = lldb.SBError()
    # units start after {len, pad} = offset 8 within the string data block
    addr = str_value.AddressOf()
    base = (addr.GetValueAsUnsigned() if addr.IsValid() else str_value.GetValueAsUnsigned()) + 8
    raw = str_value.GetProcess().ReadMemory(base, length * 2, err)
    if not err.Success() or raw is None:
        return None
    return bytes(raw).decode("utf-16-le", errors="replace")


def str_ptr_summary(valobj, _internal_dict):
    """Summary for `dream_Str *` / `dream_Str **`: renders the string as quoted text."""
    try:
        # _deref_until already lands on the dream_Str struct value
        target = _deref_until(valobj, "dream_Str")
        if target is None:
            return None
        text = _read_units(target)
        if text is None:
            return "<string: unreadable>"
        return '"' + text + '"'
    except Exception:  # a formatter must never break the debugger UI
        return None


def arr_ptr_summary(valobj, _internal_dict):
    """Summary for `dream_Arr *` / `dream_Arr **`: shows the element count."""
    try:
        target = _deref_until(valobj, "dream_Arr")
        if target is None:
            return None
        length = target.GetChildMemberWithName("len").GetValueAsSigned()
        return "len=%d" % length
    except Exception:
        return None


def __lldb_init_module(debugger, _internal_dict):
    # `-x`: the pattern is a regular expression over the full type name.
    for pattern, fn in [
        ("const dream_Str \\*\\*", "str_ptr_summary"),
        ("dream_Str \\*\\*", "str_ptr_summary"),
        ("const dream_Str \\*", "str_ptr_summary"),
        ("dream_Str \\*", "str_ptr_summary"),
        ("const dream_Arr \\*\\*", "arr_ptr_summary"),
        ("dream_Arr \\*\\*", "arr_ptr_summary"),
        ("const dream_Arr \\*", "arr_ptr_summary"),
        ("dream_Arr \\*", "arr_ptr_summary"),
    ]:
        debugger.HandleCommand(
            'type summary add -x --category Dream -F "%s.%s" "^%s$"'
            % (__name__, fn, pattern)
        )
    debugger.HandleCommand("type category enable Dream")
