#!/usr/bin/env python3
"""Exercise a packaged compiler through native execution and selective Node hosts."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from contextlib import contextmanager

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--dream', type=Path, required=True)
compiler = parser.parse_args().dream.resolve()
env = os.environ.copy()
env.pop('DREAM_LLVM', None)
env['DREAM_NO_AUTO_INSTALL'] = '1'
env['PYTHONUTF8'] = '1'

def command(args, cwd):
    result = subprocess.run([str(arg) for arg in args], cwd=cwd, env=env, text=True, encoding='utf-8', stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=180)
    if result.returncode:
        raise RuntimeError(f'{args}:\n{result.stdout}\n{result.stderr}')
    return result.stdout.replace('\r\n', '\n')

@contextmanager
def without_checkout_runtime():
    source = (root / 'runtime/src').resolve()
    if not source.is_relative_to(root.resolve()):
        raise RuntimeError('Runtime source directory is outside the checkout')
    hidden = source.with_name('src.release-check-hidden')
    if hidden.exists():
        raise RuntimeError(f'Runtime source backup already exists: {hidden}')
    source.rename(hidden)
    try:
        yield
    finally:
        hidden.rename(source)

with without_checkout_runtime(), tempfile.TemporaryDirectory(prefix='dream-release-check-') as temporary:
    work = Path(temporary)
    source = work / 'hello.dream'
    source.write_text('import system;\nfun main(): void { System.println("release 日本語"); }\n', encoding='utf-8')
    for flags in [[], ['--release']]:
        output = command([compiler, *flags, 'run', source], work)
        assert output == 'release 日本語\n', repr(output)
        print('PASS native ' + ('release' if flags else 'debug'), flush=True)
    cli_package = work / 'dream_packages' / 'cli'
    shutil.copytree(root / 'packages' / 'cli' / 'src', cli_package / 'src')
    shutil.copyfile(root / 'packages' / 'cli' / 'dream.toml', cli_package / 'dream.toml')
    cli = work / 'cli_probe.dream'
    cli.write_text('''import system;
import cli;
fun main(): void {
    let command = Command("release").string_option("message");
    let parsed = command.parse(["--message", "native ABI"]).unwrap();
    System.println(parsed.get_string("message").unwrap());
}
''', encoding='utf-8')
    assert command([compiler, '--release', 'run', cli], work) == 'native ABI\n'
    print('PASS native CLI capability ABI', flush=True)
    runner = work / 'verify.mjs'
    for package in ('csv', 'compress'):
        destination = work / 'dream_packages' / package
        shutil.copytree(root / 'packages' / package / 'src', destination / 'src')
        shutil.copyfile(root / 'packages' / package / 'dream.toml', destination / 'dream.toml')
    for name, source_name, expected in [
        ('hello', 'hello', 'Hello, world!\n'),
        ('async', 'async', 'awake\n'),
        ('typed_array', 'typed_array', '6\n'),
        ('callback', 'callback', '14\n'),
        ('csv', 'csv_probe', 'a\n'),
        ('compression', 'compression', '27\n'),
    ]:
        source = work / (source_name + '.dream')
        shutil.copyfile(root / f'tests/size/{source_name}.dream', source)
        wasm = work / (name + '.wasm')
        command([compiler, '--runtime', '--node', '-o', wasm, source], work)
        assert wasm.read_bytes().startswith(b'\0asm'), name
        assert wasm.with_suffix('.wat').is_file(), name
        runtime = wasm.with_suffix('.node.runtime.mjs')
        runner.write_text("import {pathToFileURL} from 'node:url';\nconst {run} = await import(pathToFileURL(" + json.dumps(str(runtime)) + "));\nawait run(pathToFileURL(" + json.dumps(str(wasm)) + "), {imports: {sum: values => Array.from(values).reduce((a,b) => a+b,0), invoke: callback => callback(7)}, stdout: text => process.stdout.write(text)});\n", encoding='utf-8')
        actual = command(['node', runner], work)
        assert actual == expected, (name, actual, expected)
        print('PASS Node ' + name, flush=True)
    audio_package = work / 'dream_packages' / 'audio'
    shutil.copytree(root / 'packages' / 'audio' / 'src', audio_package / 'src')
    shutil.copytree(root / 'packages' / 'audio' / 'native', audio_package / 'native')
    shutil.copyfile(root / 'packages' / 'audio' / 'dream.toml', audio_package / 'dream.toml')
    audio = work / 'audio_probe.dream'
    audio.write_text('''import audio;
async fun main(): void {
    let bytes: byte[] = [0];
    let result = AudioBuffer.decode(bytes).await;
    if result.is_err() { js.global.console.log("invalid audio"); }
}
''', encoding='utf-8')
    command([compiler, '--runtime', '--web', '-o', work / 'audio.wasm', audio], work)
    assert (work / 'audio.web.runtime.js').is_file()
    print('PASS browser audio with bundled WASI libc', flush=True)
print('Packaged compiler smoke checks passed', flush=True)
