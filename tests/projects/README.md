# Compiler assertion corpus

Run the complete corpus from the repository root:

```sh
python3 scripts/test_corpus.py
python3 scripts/test_corpus.py --node
python3 scripts/test_corpus.py --both
python3 scripts/test_corpus.py arithmetic
```

This workspace contains feature suites. Every suite builds one cached runner per target;
`dreamer test` here runs the Dream suites alone. The repository command also runs invalid
programs and process-isolated fixtures through Rust assertions in `tests/corpus/`.

New behavior tests use `@test` and `Assert.*`; keep their declarations unique within the
suite. Migrated declarations use short prefixes to avoid name collisions and excessive
monomorphized symbol lengths. Sequence checks assert individual formatted observations
and verify that every observation was consumed; they do not capture process stdout.
The corpus gives tests an explicit 180-second timeout, preserving bounded probe execution.

Invalid programs, process termination, console bytes, entrypoint/module compilation
contexts, and uninstrumented lifetime measurements belong in `tests/cases/`. Register
these with inline Rust expectations and an isolation reason. There are no expectation
sidecars. Artifact-specific compiler checks use small sources in `tests/fixtures/compiler/`.
The inventory audit rejects missing, duplicate, and unregistered corpus sources.
