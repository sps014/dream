"""Synchronize declaration sections of the API reference with the embedded Dream library.

Each page keeps its hand-written header (title, import line, guide link); everything from the
first declaration heading onward is rebuilt from the inventory. Existing code fences are reused
when their signature is unchanged so reviewed line wrapping survives a refresh.
"""

import argparse
import re
from pathlib import Path

from api_inventory import OUT, ROOT, inventory, mask, signature_key

WRAP_WIDTH = 100
KEYWORDS = r"(?:class|struct|interface|enum|fun|get|set|constructor|del)"


def declaration_name(signature):
    masked = mask(signature)
    if re.search(r"\b(?:get|set|fun)\s+this\b", masked) or re.search(r"\bthis\s*\[", masked):
        return "this"
    match = re.search(rf"\b{KEYWORDS}\b(?:\s+struct)?\s*([A-Za-z_]\w*)?", masked)
    if match:
        keyword = re.match(r"\w+", match.group(0)).group(0)
        if keyword in ("constructor", "del"):
            return keyword
        if match.group(1):
            return match.group(1)
    leading = re.search(r"(?:^|\s)([A-Za-z_]\w*)\s*(?:[:(=,]|$)", masked)
    return leading.group(1) if leading else signature.split()[0]


def split_top_level(text):
    parts, depth, start = [], 0, 0
    masked = mask(text)
    for index, char in enumerate(masked):
        if char in "([<{":
            depth += 1
        elif char in ")]>}":
            depth -= 1
        elif char == "," and depth == 0:
            parts.append(text[start:index].strip())
            start = index + 1
    tail = text[start:].strip()
    if tail:
        parts.append(tail)
    return parts


def wrap(signature):
    if len(signature) <= WRAP_WIDTH:
        return signature
    masked = mask(signature)
    open_at = masked.find("(")
    if open_at < 0:
        return signature
    depth = 0
    for index in range(open_at, len(masked)):
        depth += (masked[index] == "(") - (masked[index] == ")")
        if depth == 0:
            close_at = index
            break
    else:
        return signature
    params = split_top_level(signature[open_at + 1:close_at])
    if not params:
        return signature
    body = ",\n".join("    " + param for param in params)
    return f"{signature[:open_at + 1].rstrip()}\n{body}\n{signature[close_at:].strip()}"


def existing_fences():
    fences = {}
    for page in sorted(OUT.glob("*.md")):
        for code in re.findall(r"```dream\n(.*?)```", page.read_text(encoding="utf-8"), re.S):
            declaration = re.sub(r"\A(?:[ \t]*@[^\n]*\n)+", "", code)
            fences.setdefault(signature_key(declaration), code.rstrip("\n"))
    return fences


def header_of(path, title, package):
    if path.exists():
        text = path.read_text(encoding="utf-8")
        cut = re.search(r"(?m)^(?:## |- \[Declarations )", text)
        head = text[:cut.start()] if cut else text
        return head.rstrip("\n") + "\n\n"
    return f"# {title}\n\n**Import:** `import {package};`\n\n"


def title_of(source):
    return " ".join(word.capitalize() for word in Path(source).stem.split("_"))


def render(entries, fences):
    blocks = []
    for declaration in entries:
        code = fences.get(signature_key(declaration["signature"])) or wrap(declaration["signature"])
        blocks.append(f"## `{declaration_name(declaration['signature'])}`\n\n"
                      f"{declaration['description']}\n\n```dream\n{code}\n```\n")
    return "\n".join(blocks)


def pages():
    fences = existing_fences()
    expected = {}
    for entry in inventory():
        index_page = ROOT / "docs" / entry["page"]
        title = title_of(entry["source"])
        groups = {}
        for declaration in entry["declarations"]:
            groups.setdefault(declaration["page"], []).append(declaration)
        if len(groups) == 1:
            head = header_of(index_page, title, entry["package"])
            expected[index_page] = head + render(entry["declarations"], fences)
            continue
        head = header_of(index_page, title, entry["package"])
        links = "\n".join(f"- [Declarations {number}]({Path(page).name})"
                          for number, page in enumerate(groups, start=1))
        expected[index_page] = head + links + "\n"
        for page, declarations in groups.items():
            path = ROOT / "docs" / page
            source = path if path.exists() else index_page
            expected[path] = header_of(source, title, entry["package"]) + render(declarations, fences)
    return expected


def stale(expected):
    owned = set(expected)
    result = []
    for path in sorted(OUT.glob("*.md")):
        if path in owned:
            continue
        text = path.read_text(encoding="utf-8")
        if text.startswith("# ") and "](index.md#reading-a-signature)" in text:
            result.append(path)
    return result


def relink(removed, expected):
    """Point links at a collapsed `name-N.md` part page to the surviving `name.md` page."""
    targets = {}
    for path in removed:
        merged = OUT / re.sub(r"-\d+\.md$", ".md", path.name)
        if merged != path and merged in expected:
            targets[path.name] = merged.name
    if not targets:
        return []
    pattern = re.compile(r"\]\((" + "|".join(re.escape(name) for name in targets) + r")(#[^)]*)?\)")
    changed = []
    for page in sorted((ROOT / "docs").rglob("*.md")):
        text = expected.get(page) if page in expected else page.read_text(encoding="utf-8")
        updated = pattern.sub(lambda m: f"]({targets[m.group(1)]}{m.group(2) or ''})", text)
        if updated != text:
            changed.append((page, updated))
    return changed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail when a page differs from the library")
    args = parser.parse_args()
    expected = pages()
    changed = [path for path, text in expected.items()
               if not path.exists() or path.read_text(encoding="utf-8") != text]
    removed = stale(expected)
    if args.check:
        if changed or removed:
            names = ", ".join(path.name for path in changed + removed)
            raise SystemExit(f"API reference pages are out of date: {names}. Run api_pages.py.")
        print(f"API reference: {len(expected)} pages in sync.")
        return
    for path in changed:
        path.write_text(expected[path], encoding="utf-8", newline="\n")
    for path, text in relink(removed, expected):
        path.write_text(text, encoding="utf-8", newline="\n")
    for path in removed:
        path.unlink()
    print(f"API reference: {len(changed)} pages written, {len(removed)} removed.")


if __name__ == "__main__":
    main()
