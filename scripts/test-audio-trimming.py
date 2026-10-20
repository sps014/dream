#!/usr/bin/env python3
"""Verify audio reachability and deployment isolation using the same source/output paths."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, default=ROOT / "target/debug/dream.exe")
    parser.add_argument("--native", action="store_true", help="also check native relocatable deployments")
    parser.add_argument("--native-only", action="store_true", help="check only native relocatable deployments")
    args = parser.parse_args()
    directory = ROOT / "target/audio-trimming"
    directory.mkdir(parents=True, exist_ok=True)
    source = directory / "main.dream"
    output = directory / "main.wasm"
    baseline = None
    cases = {
        "baseline": "async fun main(): void {}",
        "unused import": "import system.audio; async fun main(): void {}",
        "dead calls": "import system.audio; async fun main(): void { if false { let audio = Audio(\"unused.mp3\"); audio.play().await; } }",
        "analysis": "import system.audio; fun main(): void { let analyzer = AudioAnalyzer(); analyzer.analyze([0.0f], 48000, 1); }",
        "PCM playback": "import system.audio; async fun main(): void { let buffer = AudioBuffer.from_pcm([0.0f], 48000, 1).unwrap(); let audio = Audio(buffer); audio.play().await; audio.close(); }",
        "capture": "import system.audio; async fun main(): void { let buffer = AudioBuffer.from_pcm([0.0f], 48000, 1).unwrap(); let audio = Audio(buffer); audio.play().await; audio.capture(2048); audio.close(); }",
        "decode": "import system.audio; fun main(): void { AudioBuffer.decode([(byte)1]); }",
        "rebuilt without audio": "async fun main(): void {}",
    }
    for label, text in (() if args.native_only else cases.items()):
        source.write_text(text + "\n", encoding="utf-8")
        process = subprocess.run([str(args.compiler.resolve()), "--web", str(source), "-o", str(output)],
                                 cwd=ROOT, capture_output=True, text=True)
        if process.returncode:
            raise RuntimeError(process.stdout + process.stderr)
        artifacts = {ext: output.with_suffix("." + ext).read_bytes()
                     for ext in ("wasm", "wat", "abi.json", "web.runtime.js")}
        if label == "baseline":
            baseline = artifacts
        elif label in ("unused import", "dead calls", "rebuilt without audio"):
            for ext, data in artifacts.items():
                assert data == baseline[ext], f"{label} changed {ext}"
        wat = artifacts["wat"].decode()
        runtime = artifacts["web.runtime.js"].decode()
        abi = json.loads(artifacts["abi.json"])
        if label != "decode":
            assert "drwav_" not in wat and "drmp3_" not in wat, f"{label} retained decoders"
        for name in ("dream_audio_stream_", "nestegg_", "opus_decode"):
            if label != "decode" or name == "dream_audio_stream_":
                assert name not in wat, f"{label} retained {name}"
        if label != "analysis":
            assert "kiss_fft" not in wat, f"{label} retained FFT"
        if label != "capture":
            assert "dream-audio-capture" not in runtime, f"{label} retained capture"
        if label not in ("PCM playback", "capture"):
            assert "AudioWorklet" not in runtime, f"{label} retained playback"
            assert not any(entry.get("field", "").startswith("audioOutput")
                           for entry in abi.get("externs", [])), f"{label} retained output imports"
        print(f"{label}: passed ({len(artifacts['wasm'])} WASM bytes)")
    if args.native or args.native_only:
        directory = directory / "native"
        directory.mkdir(exist_ok=True)
        source = directory / "main.dream"
        output = directory / "main.bin"
        baseline = None
        for label in ("baseline", "unused import", "dead calls", "PCM playback", "rebuilt without audio"):
            source.write_text(cases[label] + "\n", encoding="utf-8")
            process = subprocess.run([str(args.compiler.resolve()), "--release", "--relocatable", str(source), "-o", str(output.with_suffix(".ll"))],
                                     cwd=ROOT, capture_output=True, text=True)
            if process.returncode:
                raise RuntimeError(process.stdout + process.stderr)
            artifacts = {ext: output.with_suffix("." + ext).read_bytes() for ext in ("bin", "abi.json")}
            if label == "baseline":
                baseline = artifacts
            if label in ("unused import", "dead calls", "rebuilt without audio"):
                assert artifacts == baseline, f"native {label} changed deployment artifacts"
            libraries = list(directory.glob("*dream_host_audio*"))
            assert bool(libraries) == (label == "PCM playback"), f"native {label}: unexpected audio libraries"
            print(f"native {label}: passed ({len(artifacts['bin'])} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
