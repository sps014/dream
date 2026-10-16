"""Check the published declaration inventory against the embedded Dream library.

Pygments supplies lexical boundaries; this reads declarations, not function bodies.
It deliberately does not try to evaluate attributes or infer platform behavior.
"""

import argparse
import json
import re
from pathlib import Path

from pygments.token import Comment, String, Whitespace

from dream_lexer import DreamLexer

ROOT = Path(__file__).resolve().parents[2]
STDLIB = ROOT / "crates/dream-stdlib/src/system"
MANIFEST = ROOT / "tooling/docs/api-coverage.json"
OUT = ROOT / "docs/reference/api"


def mask(source):
    result = list(source)
    for offset, kind, value in DreamLexer().get_tokens_unprocessed(source):
        if kind in Comment or kind in String:
            for index in range(offset, offset + len(value)):
                if result[index] != "\n":
                    result[index] = " "
    return "".join(result)


def comment_before(source, offset):
    lines = source[:offset].splitlines()
    if lines and not lines[-1].strip():
        lines.pop()
    comments = []
    docs = []
    attribute_depth = 0
    for line in reversed(lines):
        line = line.strip()
        if line.startswith("@") or attribute_depth or line == ")":
            lexical = mask(line)
            attribute_depth += lexical.count(")") - lexical.count("(")
            continue
        if line.startswith("//"):
            comments.append(line.lstrip("/").strip())
            if line.startswith("///"):
                docs.append(line[3:].strip())
        else:
            break
    return " ".join(reversed(docs or comments))


def signature_key(source):
    """Ignore layout whitespace without changing spaces inside string defaults."""
    return "".join(value for _, kind, value in DreamLexer().get_tokens_unprocessed(source)
                   if kind not in Comment and kind not in Whitespace)


def signature_end(masked, start):
    parentheses = brackets = 0
    for index in range(start, len(masked)):
        char = masked[index]
        if char == "(":
            parentheses += 1
        elif char == ")":
            parentheses -= 1
        elif char == "[":
            brackets += 1
        elif char == "]":
            brackets -= 1
        elif char in "{;" and not parentheses and not brackets:
            return index
    raise ValueError(f"Unterminated declaration at {start}")


def declarations(source):
    masked = mask(source)
    entries = []
    private_ranges = []
    for owner in re.finditer(r"(?m)^(?!(?:public|extend|shared\s+public)\b)(?:(?:shared|internal|static|ref|unmanaged|sealed|abstract)\s+)*(?:class|struct|interface|enum)\s+\w+[^\{;]*\{", masked):
        end = owner.end()
        depth = 1
        while depth and end < len(masked):
            depth += (masked[end] == "{") - (masked[end] == "}")
            end += 1
        private_ranges.append((owner.start(), end))
    pattern = r"(?m)^[ \t]*(?:shared\s+)?public\s+[^\n]+"
    for match in re.finditer(pattern, masked):
        if any(start <= match.start() < end for start, end in private_ranges):
            continue
        start = match.start() + len(match.group()) - len(match.group().lstrip())
        end = signature_end(masked, start)
        signature = " ".join(source[start:end].split())
        if not signature:
            continue
        entries.append({"signature": signature, "description": comment_before(source, match.start()),
                        "line": source.count("\n", 0, start) + 1})
    # Interface requirements have public visibility without a written modifier.
    for match in re.finditer(r"(?m)^public\s+interface\s+[^\{]+\{", masked):
        start = match.end()
        depth = 1
        end = start
        while depth and end < len(masked):
            depth += (masked[end] == "{") - (masked[end] == "}")
            end += 1
        for member in re.finditer(r"(?m)^    (?:(?:borrow|unique|async|static)\s+)*(?:fun|get|set)\s+[^\n]+", masked[start:end - 1]):
            offset = start + member.start() + 4
            stop = signature_end(masked, offset)
            entries.append({"signature": " ".join(source[offset:stop].split()),
                            "description": comment_before(source, offset - 4),
                            "line": source.count("\n", 0, offset) + 1})
    for match in re.finditer(r"(?m)^public\s+enum(?:\s+struct)?\s+\w+[^\{]*\{", masked):
        start = match.end()
        end = masked.find("}", start)
        for variant in re.finditer(r"(?m)^    (\w+(?:\([^\n]*\))?(?:\s*=\s*[^,\n]+)?)\s*,?\s*$", source[start:end]):
            offset = start + variant.start()
            entries.append({"signature": variant.group(1).strip(),
                            "description": comment_before(source, offset),
                            "line": source.count("\n", 0, offset) + 1})
    return sorted(entries, key=lambda item: item["line"])


def inventory():
    registry = "\n".join(path.read_text(encoding="utf-8") for path in sorted((ROOT / "crates/dream-stdlib/src/registry").glob("*.rs")))
    sources = sorted(set(re.findall(r'include_str!\("(?:\.\./)?(system/[^\"]+\.dream)"\)', registry)))
    result = []
    for relative in sources:
        path = ROOT / "crates/dream-stdlib/src" / relative
        source = path.read_text(encoding="utf-8")
        package = re.search(r"^module\s+([\w.]+);", source, re.M)
        entries = declarations(source)
        if entries:
            slug = relative.removeprefix("system/").removesuffix(".dream").replace("/", "-").replace("_", "-")
            for index, declaration in enumerate(entries):
                declaration["page"] = f"reference/api/{slug}.md" if len(entries) <= 24 else f"reference/api/{slug}-{index // 24 + 1}.md"
            result.append({"source": path.relative_to(ROOT).as_posix(),
                           "package": package.group(1) if package else "system",
                           "page": f"reference/api/{slug}.md",
                           "declarations": entries})
    return result


def check(current):
    if not MANIFEST.exists():
        raise SystemExit("Missing API inventory. Run api_inventory.py --write.")
    saved = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if current != saved:
        raise SystemExit("Public library declarations changed. Review the reference and refresh the API inventory.")
    pages = {}
    for entry in saved:
        for declaration in entry["declarations"]:
            if not declaration["description"]:
                raise SystemExit(f"Undocumented API in {entry['source']}:{declaration['line']}: {declaration['signature']}")
            page = declaration["page"]
            if page not in pages:
                text = (ROOT / "docs" / page).read_text(encoding="utf-8")
                fences = re.findall(r"```dream\n(.*?)```", text, re.S)
                pages[page] = (" ".join(text.split()), [signature_key(code) for code in fences])
            prose, signatures = pages[page]
            if " ".join(declaration["description"].split()) not in prose:
                raise SystemExit(f"Missing API description in {page}: {declaration['signature']}")
            if not any(signature_key(declaration["signature"]) in code for code in signatures):
                raise SystemExit(f"Missing declaration in {entry['page']}: {declaration['signature']}")
    total = sum(len(entry["declarations"]) for entry in saved)
    print(f"API inventory: {total} declarations in {len(saved)} source files checked.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="Refresh declaration inventory after reviewing the reference")
    args = parser.parse_args()
    current = inventory()
    if args.write:
        MANIFEST.write_text(json.dumps(current, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    else:
        check(current)


if __name__ == "__main__":
    main()
