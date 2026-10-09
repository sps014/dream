"""Cold, unchanged and edited generator builds with an isolated cache."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from benchmarks.process import dream_name, with_exe

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target" / "release" / dream_name())
    parser.add_argument("--save", type=Path)
    parser.add_argument("--check", type=Path)
    args = parser.parse_args()
    args.binary = with_exe(args.binary)
    if not args.binary.is_file():
        parser.error("build the compiler first: cargo build --release --workspace")
    small = (ROOT / "tests/cases/json_derive.dream").read_text()
    models = "\n".join(f"""@json
class Model{i} {{
    public id: int;
    public name: string;
    public tags: List<string>;
    public score: Option<double>;
    public constructor(id: int, name: string, tags: List<string>, score: Option<double>) {{
        this.id = id; this.name = name; this.tags = tags; this.score = score;
    }}
}}""" for i in range(200))
    scenarios = {
        "no_json": "import system; fun main() { System.println(42); }",
        "small_json": small,
        "big_json": ("import system; import system.json; import system.collections;\n" + models
                     + "\nfun main() { System.println(Json.serialize(List<Model0>())); }"),
    }
    rows = {}
    with tempfile.TemporaryDirectory(prefix="dream-generators-") as directory:
        work = Path(directory)
        for scenario, source in scenarios.items():
            project = work / scenario
            project.mkdir()
            path = project / "main.dream"
            path.write_text(source)
            # A private prefix isolates generator caches without deleting the developer's cache.
            env = dict(os.environ, DREAM_PREFIX=str(project / "prefix"))
            env.setdefault("DREAM_TOOLCHAINS", str(Path(os.environ.get("DREAM_PREFIX", Path.home() / ".dream")) / "toolchains"))
            for phase in ("cold", "warm", "one_edit"):
                if phase == "one_edit":
                    path.write_text(source + "\nfun bench_edit_marker(): int { return 1; }\n")
                env["DREAM_BENCH_NONCE"] = str(time.monotonic_ns())
                started = time.monotonic()
                result = subprocess.run([str(args.binary.resolve()), "--emit-llvm", str(path)],
                                        cwd=ROOT, env=env, capture_output=True, text=True)
                if result.returncode:
                    raise ValueError(result.stdout + result.stderr)
                elapsed = time.monotonic() - started
                rows[f"{scenario}/{phase}"] = elapsed
                print(f"{scenario:<12} {phase:<10} {elapsed:.3f}s")
    if args.save:
        args.save.parent.mkdir(parents=True, exist_ok=True)
        args.save.write_text(json.dumps(rows, indent=2) + "\n")
    if args.check:
        reference = json.loads(args.check.read_text())
        if reference.keys() != rows.keys():
            raise ValueError("generator reference has different scenarios")
        return int(any(value > reference[key] * 1.15 + 0.05 for key, value in rows.items()))
    return 0
