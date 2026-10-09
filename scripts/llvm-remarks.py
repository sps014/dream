#!/usr/bin/env python3
"""Group first-pass LLVM YAML remarks by function, pass, reason and clobber/callee."""
import argparse
from collections import Counter
import json
from pathlib import Path

import yaml


class RemarkLoader(getattr(yaml, "CSafeLoader", yaml.SafeLoader)):
    pass


def remark(loader, suffix, node):
    value = loader.construct_mapping(node, deep=True)
    value["Kind"] = suffix
    return value


RemarkLoader.add_multi_constructor("!", remark)


def summarize(paths):
    groups = Counter()
    for path in paths:
        with Path(path).open() as source:
            for value in yaml.load_all(source, Loader=RemarkLoader):
                if not value:
                    continue
                details = []
                for argument in value.get("Args", []):
                    details.extend(f"{key}={item}" for key, item in argument.items()
                                   if key != "DebugLoc")
                groups[(value.get("Function", "?"), value.get("Pass", "?"),
                        value["Kind"], value.get("Name", "?"), " ".join(details))] += 1
    return [dict(function=fn, pass_name=pass_name, kind=kind, reason=reason,
                 details=details, count=count)
            for (fn, pass_name, kind, reason, details), count in sorted(groups.items())]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = json.dumps(summarize(args.paths), indent=2) + "\n"
    if args.output:
        args.output.write_text(result)
    else:
        print(result, end="")


if __name__ == "__main__":
    main()
