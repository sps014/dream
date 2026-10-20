#!/usr/bin/env python3
"""Exercise a packaged compiler through native execution and selective Node hosts."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

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

with tempfile.TemporaryDirectory(prefix='dream-release-check-') as temporary:
    work = Path(temporary)
    source = work / 'hello.dream'
    source.write_text('import system;\nfun main(): void { System.println("release 日本語"); }\n', encoding='utf-8')
    for flags in [[], ['--release']]:
        output = command([compiler, *flags, 'run', source], work)
        assert output == 'release 日本語\n', repr(output)
        print('PASS native ' + ('release' if flags else 'debug'), flush=True)
    runner = work / 'verify.mjs'
    for name, expected in [('hello', 'Hello, world!\n'), ('async', 'awake\n'), ('typed_array', '6\n'), ('callback', '14\n'), ('csv', 'a\n'), ('compression', '27\n')]:
        source = work / (name + '.dream')
        shutil.copyfile(root / f'tests/size/{name}.dream', source)
        wasm = work / (name + '.wasm')
        command([compiler, '--runtime', '--node', '-o', wasm, source], work)
        assert wasm.read_bytes().startswith(b'\0asm'), name
        assert wasm.with_suffix('.wat').is_file(), name
        runtime = wasm.with_suffix('.node.runtime.js')
        runner.write_text("import {pathToFileURL} from 'node:url';\nconst {run} = await import(pathToFileURL(" + json.dumps(str(runtime)) + "));\nawait run(pathToFileURL(" + json.dumps(str(wasm)) + "), {imports: {sum: values => Array.from(values).reduce((a,b) => a+b,0), invoke: callback => callback(7)}, stdout: text => process.stdout.write(text)});\n", encoding='utf-8')
        actual = command(['node', runner], work)
        assert actual == expected, (name, actual, expected)
        if name in ['csv', 'compression']:
            assert wasm.with_suffix('.' + name + '.wasm').is_file()
        print('PASS Node ' + name, flush=True)
print('Packaged compiler smoke checks passed', flush=True)
