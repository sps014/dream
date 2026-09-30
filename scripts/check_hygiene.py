#!/usr/bin/env python3
"""Ratchet structural compiler hygiene without pretending the legacy baseline is clean."""

from __future__ import annotations

from collections import Counter
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
MAX_LARGE_RUST_FILES = 57

# These are symbol construction or LLVM attribute parsing, not semantic decisions. Keeping the
# exact fingerprints here makes any new string-based decision fail CI until it is reviewed.
ALLOWED_STRING_PATTERNS = Counter(
    {
        (
            "crates/dream-mir/src/backend/llvm/runtime_sigs.rs",
            'if body.is_some_and(|b| b.contains("\\\"wasm-export-name\\\"")) {',
        ): 1,
        (
            "crates/dream-mir/src/backend/llvm/lcx.rs",
            'super::ir::fmt::global(&format!("{}_blk", self.cx.str_sym(s))),',
        ): 1,
        (
            "crates/dream-mir/src/backend/shared/abi_types.rs",
            'Some(format!("{}_poll", import_call_name(imp)))',
        ): 1,
        (
            "crates/dream-mir/src/backend/shared/protocol_names.rs",
            'c_ident(&format!("{}_to_string", l.name))',
        ): 1,
        (
            "crates/dream-mir/src/backend/shared/protocol_names.rs",
            'c_ident(&format!("{}_to_string", u.name))',
        ): 1,
        (
            "crates/dream-mir/src/backend/shared/protocol_names.rs",
            'c_ident(&format!("{}_hash_code", l.name))',
        ): 1,
        (
            "crates/dream-mir/src/backend/shared/protocol_names.rs",
            'c_ident(&format!("{}_hash_code", u.name))',
        ): 1,
        (
            "crates/dream-mir/src/backend/shared/symbols.rs",
            'format!("{}__{}", func.name, args.join("_"))',
        ): 1,
        (
            "crates/dream-mir/src/backend/llvm/glue/release.rs",
            'add(l, format!("{}_into", arr_rel(*e)));',
        ): 1,
        (
            "crates/dream-mir/src/backend/llvm/glue/release.rs",
            'let tail = format!("{}_into", arr_rel(*e));',
        ): 1,
    }
)


def is_test_file(path: Path) -> bool:
    rel = path.relative_to(ROOT)
    return "tests" in rel.parts or path.name == "tests.rs" or path.stem.endswith("_tests")


def production_rust_files() -> list[Path]:
    roots = [ROOT / "build.rs", ROOT / "src", ROOT / "crates", ROOT / "tooling"]
    files: list[Path] = []
    for base in roots:
        candidates = [base] if base.is_file() else base.rglob("*.rs")
        files.extend(path for path in candidates if not is_test_file(path))
    return sorted(files)


def line_count(path: Path) -> int:
    with path.open(encoding="utf-8", errors="replace") as source:
        return sum(1 for _ in source)


def string_pattern_hits() -> Counter[tuple[str, str]]:
    hits: Counter[tuple[str, str]] = Counter()
    roots = [ROOT / "crates/dream-mir/src/passes", ROOT / "crates/dream-mir/src/backend"]
    for base in roots:
        for path in base.rglob("*.rs"):
            if is_test_file(path):
                continue
            production = path.read_text(encoding="utf-8").split("\n#[cfg(test)]", 1)[0]
            rel = path.relative_to(ROOT).as_posix()
            for line in production.splitlines():
                stripped = line.strip()
                if 'contains("' in stripped or 'format!("{}_' in stripped:
                    hits[(rel, stripped)] += 1
    return hits


def main() -> int:
    large = [(path, line_count(path)) for path in production_rust_files()]
    large = sorted(((path, count) for path, count in large if count > 600), key=lambda x: -x[1])
    print(f"production Rust files over 600 lines: {len(large)} (baseline {MAX_LARGE_RUST_FILES})")
    for path, count in large:
        print(f"  {count:5} {path.relative_to(ROOT)}")

    failures: list[str] = []
    if len(large) > MAX_LARGE_RUST_FILES:
        failures.append(
            f"large production Rust files increased from {MAX_LARGE_RUST_FILES} to {len(large)}"
        )

    unexpected = string_pattern_hits() - ALLOWED_STRING_PATTERNS
    for (path, line), count in unexpected.items():
        failures.append(f"new name-string pattern ({count}x): {path}: {line}")

    if failures:
        print("\nhygiene ratchet failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
