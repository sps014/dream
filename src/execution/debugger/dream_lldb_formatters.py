"""LLDB formatters for Dream debug sessions.

Auto-imported by `dream debug-adapter` (see src/execution/debugger/mod.rs) so strings render as
quoted text, arrays as [e0, e1, ...], and discriminated unions as the active variant
(`Circle(radius=12)`). Everything here is presentation-only.
"""

import lldb

MAX_ELEMS = 64
MAX_DEPTH = 2
_process = None


def _pointee_struct(value):
    """Dereferences `value` until it stops being a pointer; None if unreachable."""
    seen = 0
    while value.TypeIsPointerType() and seen < 4:
        value = value.Dereference()
        if value is None or not value.IsValid():
            return None
        seen += 1
    if value.IsValid() and not value.TypeIsPointerType():
        return value
    return None


def _read_str(process, addr):
    """Reads a dream_Str block at `addr` and decodes its UTF-16 units."""
    err = lldb.SBError()
    raw_len = process.ReadMemory(addr, 4, err)
    if not err.Success() or raw_len is None:
        return None
    length = int.from_bytes(raw_len, "little", signed=True)
    if length < 0:
        return None
    if length == 0:
        return ""
    raw = process.ReadMemory(addr + 8, length * 2, err)
    if not err.Success() or raw is None:
        return None
    return '"' + bytes(raw).decode("utf-16-le", errors="replace") + '"'


def _elem_text(child, process, depth=0):
    """Renders one array element child; falls back to its own summary/value."""
    tname = (child.GetType().GetName() or "").replace("const ", "").strip()
    if "dream_Str" in tname:
        addr = child.GetValueAsUnsigned()
        if addr == 0:
            return "null"
        text = _read_str(process, addr)
        return text if text is not None else "<str>"
    # Nested Dream views (arrays, unions, class instances) render recursively, depth-capped so
    # self-referential structures terminate.
    if depth < MAX_DEPTH and (
        "dream_Arr" in tname
        or "*" not in tname.replace("**", "*").rstrip("*").strip()
    ):
        inner = child.Dereference() if child.TypeIsPointerType() else child
        if inner is not None and inner.IsValid():
            rendered = _dispatch(inner, depth + 1)
            if rendered is not None:
                return rendered
    summary = child.GetSummary()
    if summary:
        return summary
    value = child.GetValue()
    return value if value is not None else "?"


def _scalar_text(name, raw):
    """Renders little-endian raw bytes for a scalar element spelling."""
    try:
        if name in ("int32_t", "int"):
            return str(int.from_bytes(raw, "little", signed=True))
        if name in ("uint32_t", "unsigned"):
            return str(int.from_bytes(raw, "little", signed=False))
        if name == "int64_t":
            return str(int.from_bytes(raw, "little", signed=True))
        if name == "float":
            import struct as _struct
            return repr(_struct.unpack("<f", raw)[0])
        if name == "double":
            import struct as _struct
            return repr(_struct.unpack("<d", raw)[0])
        if name.startswith("dream_Str"):
            addr = int.from_bytes(raw, "little")
            text = _read_str(_process, addr) if addr else "null"
            return text if text is not None else "<str>"
        if name.endswith("*"):
            return "0x%x" % int.from_bytes(raw, "little")
    except Exception:
        pass
    return "<e>"


def _array_summary(struct_value, depth=0):
    length = max(struct_value.GetChildMemberWithName("len").GetValueAsSigned(), 0)
    elems = struct_value.GetChildMemberWithName("elems")
    if not elems.IsValid():
        # Untyped fallback shape (`bytes[]`): only the count is meaningful.
        return "len=%d" % length
    elem_ty = elems.GetType().GetArrayElementType()
    if elem_ty is None or not elem_ty.IsValid():
        return "len=%d" % length
    esize = elem_ty.GetByteSize() or 8
    ename = (elem_ty.GetName() or "").replace("const ", "").strip()
    err = lldb.SBError()
    base = struct_value.AddressOf().GetValueAsUnsigned() + 4
    process = struct_value.GetProcess()
    global _process
    _process = process
    parts = []
    shown = min(length, MAX_ELEMS)
    for i in range(shown):
        raw = process.ReadMemory(base + i * esize, esize, err)
        if not err.Success() or raw is None:
            parts.append("?")
            continue
        # Nested arrays recurse through a synthetic pointer to this element; class/object
        # elements render as identities (expanding them in the UI shows full DWARF detail).
        if ename.startswith("dream_Arr"):
            child = struct_value.CreateValueFromAddress(
                "%d" % i,
                base + i * esize,
                elem_ty.GetPointerType(),
            )
            if child.IsValid():
                inner = child.Dereference()
                if inner is not None and inner.IsValid():
                    rendered = _dispatch(inner, depth + 1)
                    parts.append(rendered if rendered is not None else "[?]")
                    continue
            parts.append("[?]")
            continue
        if ename.endswith("*"):
            addr = int.from_bytes(raw, "little")
            base_name = ename.replace("*", "").strip()
            parts.append("null" if addr == 0 else "%s@0x%x" % (base_name, addr))
            continue
        parts.append(_scalar_text(ename, bytes(raw)))
    if length > MAX_ELEMS:
        parts.append("...")
    return "[" + ", ".join(parts) + "]"


def _union_summary(struct_value, depth=0):
    idx = struct_value.GetChildMemberWithName("tag").GetValueAsSigned()
    # Views always name the payload union member `value`; its children are the variant
    # structs in declaration order, so the tag indexes them directly.
    variants = struct_value.GetChildMemberWithName("value")
    if not variants.IsValid():
        return None
    active = variants.GetChildAtIndex(idx)
    if not active.IsValid():
        return "<invalid variant %d>" % idx
    name = active.GetName() or "?"
    fields = []
    for i in range(active.GetNumChildren()):
        f = active.GetChildAtIndex(i)
        if not f.IsValid():
            continue
        rendered = _elem_text(f, struct_value.GetProcess(), depth)
        fname = f.GetName() or ""
        fields.append("%s=%s" % (fname, rendered) if rendered and not rendered.startswith('"') else rendered)
    return "%s(%s)" % (name, ", ".join(fields)) if fields else name


def _string_summary(struct_value):
    addr = struct_value.AddressOf()
    base = addr.GetValueAsUnsigned() if addr.IsValid() else struct_value.GetValueAsUnsigned()
    text = _read_str(struct_value.GetProcess(), base)
    return text if text is not None else "<string: unreadable>"


def dream_view_summary(valobj, _internal_dict, _options=None):
    """Dispatcher registered for pointer-to-named-type views. Returns None for types it does
    not recognize so lldb falls back to default display.

    lldb may pass a third SBTypeSummaryOptions argument; it must never land on `depth`, so the
    recursive entry point is kept separate from the registered one.
    """
    return _dispatch(valobj, 0)


def str_ptr_summary(valobj, internal_dict):
    return dream_view_summary(valobj, internal_dict)


def arr_ptr_summary(valobj, internal_dict):
    return dream_view_summary(valobj, internal_dict)


def view_ptr_summary(valobj, internal_dict):
    return dream_view_summary(valobj, internal_dict)


def _dispatch(valobj, depth=0):
    try:
        if depth > MAX_DEPTH:
            return None
        s = _pointee_struct(valobj)
        if s is None:
            return None
        name = (s.GetType().GetName() or "").replace("const ", "").strip()
        if name == "dream_Str":
            return _string_summary(s)
        if name.startswith("dream_Arr"):
            return _array_summary(s, depth)
        if name == "tag":
            return None
        if (
            s.GetChildMemberWithName("tag").IsValid()
            and s.GetChildMemberWithName("value").IsValid()
        ):
            return _union_summary(s, depth)
        # Nominal class/value-struct views render compactly as Name{f=v, ...}.
        n = s.GetNumChildren()
        if n > 0 and name[:1].isupper():
            parts = []
            for i in range(min(n, 8)):
                f = s.GetChildAtIndex(i)
                if f.IsValid():
                    fname = f.GetName() or "?"
                    parts.append(
                        "%s=%s" % (fname, _elem_text(f, s.GetProcess(), depth))
                    )
            if parts:
                return "%s{%s}" % (name, ", ".join(parts))
        return None
    except Exception:  # a formatter must never break the debugger UI
        return None


def arr_ptr_summary(valobj, internal_dict):
    return dream_view_summary(valobj, internal_dict)


def __lldb_init_module(debugger, _internal_dict):
    # Type names are program-specific, so the adapter writes them next to this file as
    # `<stem>_lldb_names.py` (`NAMES = [...]`). Registration happens here, in the import hook,
    # which is the one path that provably attaches summaries for the session.
    import os

    base = os.path.splitext(__file__)[0]
    names_path = base.replace("_lldb_dream", "_lldb_names") + ".py"
    names = []
    if os.path.exists(names_path):
        scope: dict = {}
        exec(compile(open(names_path).read(), names_path, "exec"), scope)
        names = scope.get("NAMES", [])
    for name in names:
        for pattern in (
            "^const %s \\*\\*$" % name,
            "^%s \\*\\*$" % name,
            "^const %s \\*$" % name,
            "^%s \\*$" % name,
        ):
            debugger.HandleCommand(
                'type summary add -x --category Dream -F "%s.dream_view_summary" "%s"'
                % (__name__, pattern)
            )
    debugger.HandleCommand("type category enable Dream")
