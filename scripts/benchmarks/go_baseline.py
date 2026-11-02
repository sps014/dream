#!/usr/bin/env python3
"""Measure `go build` on programs shaped like the `scripts/bench.py build` workloads."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time

HELLO = '''package main

import "fmt"

type Node struct {
	value int
}

func main() {
	node := &Node{value: 42}
	fmt.Println(node.value)
}
'''
STDLIB = '''package main

import (
	"encoding/json"
	"fmt"
	"regexp"
)

type Developer struct {
	Name      string   `json:"name"`
	Languages []string `json:"languages"`
}

func main() {
	developers := []*Developer{
		{Name: "Ada", Languages: []string{"Dream", "TypeScript"}},
		{Name: "Grace", Languages: []string{"COBOL"}},
	}
	counts := map[string]int{}
	digits := regexp.MustCompile(`\\d+`)
	for _, developer := range developers {
		text, _ := json.Marshal(developer)
		counts[developer.Name] = len(text)
		fmt.Println(string(text))
	}
	var parsed map[string]any
	_ = json.Unmarshal([]byte(`{"seed": 42}`), &parsed)
	fmt.Println(parsed["seed"])
	fmt.Println(digits.MatchString("abc123"))
	fmt.Println(len(counts))
}
'''


def generated(functions):
    """Mirrors `generated_source` in `scripts/benchmarks/build.py`."""
    parts = ['package main\n\nimport "fmt"\n\ntype Node struct {\n\tvalue int\n}\n\n']
    for i in range(functions):
        parts.append(f'''//go:noinline
func work_{i}(seed int) int {{
	node := &Node{{value: seed}}
	value := node.value
	for i := 0; i < 32; i++ {{
		if i%3 == 0 {{
			value += i
		}} else {{
			value -= i
		}}
	}}
	return value
}}

''')
    calls = "\n".join(f"\ttotal += work_{i}({i})" for i in range(functions))
    parts.append(f"func main() {{\n\ttotal := (&Node{{value: 42}}).value\n{calls}\n\tfmt.Println(total)\n}}\n")
    return "".join(parts)


def workload(name):
    if name == "hello":
        return HELLO, "value: 42}"
    if name == "stdlib":
        return STDLIB, '"seed": 42'
    match = re.fullmatch(r"gen(\d+)", name)
    if match and int(match[1]) > 0:
        return generated(int(match[1])), "value: 42}"
    raise argparse.ArgumentTypeError(f"unknown workload {name!r}: use hello, genN or stdlib")


PROFILES = {"default": [], "noopt": ["-gcflags=all=-N -l"]}


def run(command, cwd, env):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, text=True)
    seconds = time.monotonic() - started
    if result.returncode:
        raise RuntimeError(result.stdout + result.stderr)
    return seconds


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go", default=shutil.which("go") or "go")
    parser.add_argument("--repeat", type=int, default=5)
    parser.add_argument("--workload", default="hello")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        source_text, marker = workload(args.workload)
    except argparse.ArgumentTypeError as error:
        parser.error(str(error))
    edited_text = source_text.replace(marker, marker.replace("42", "43"), 1)
    version = subprocess.run([args.go, "version"], capture_output=True, text=True, check=True).stdout.strip()
    rows = []
    with tempfile.TemporaryDirectory(prefix="go-build-bench-") as directory:
        work = Path(directory)
        for profile, flags in PROFILES.items():
            for repeat in range(args.repeat):
                project = work / f"{profile}-{repeat}"
                project.mkdir()
                (project / "go.mod").write_text("module bench\n\ngo 1.21\n")
                source = project / "main.go"
                source.write_text(source_text)
                # A fresh GOCACHE also recompiles the Go standard library, the counterpart of
                # Dream's cold runtime build.
                env = dict(os.environ, GOCACHE=str(project / "cache"), GOFLAGS="", GOTOOLCHAIN="local",
                           GOTELEMETRY="off", CGO_ENABLED="0")
                command = [args.go, "build", *flags, "-o", str(project / "main.bin"), "."]
                for phase in ["cold", "unchanged", "edited", "unchanged_after_edit"]:
                    if phase == "edited":
                        source.write_text(edited_text)
                    seconds = run(command, project, env)
                    rows.append(dict(profile=profile, repeat=repeat, phase=phase, seconds=seconds))
                    print(f"go-{profile}-{repeat}-{phase}: {seconds:.3f}s", flush=True)
    destination = args.output.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps({
        "platform": platform.platform(), "go": version, "workload": args.workload,
        "source_lines": source_text.count("\n"), "rows": rows,
    }, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
