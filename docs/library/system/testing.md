---
title: Testing
summary: "Write `@test` functions with assertions."
icon: material/check-decagram-outline
api: reference/api/system-testing.md
---

# Testing


Tests describe behavior you want to keep working. Mark a function with `@test`, use assertions to check its results, and run it with `dreamer test` or `dream test`. A failed assertion stops the test and reports the problem.

**Import:** `import system.testing;`

Mark tests with `@test`, then run `dream test` or `dreamer test`. A failed assertion prints a message and exits (no exceptions).

```dream
import system;
import system.testing;

@test
fun addition_works(): void {
    Assert.eq(2 + 2, 4);
}
```

```bash
dream test path/to/file.dream
dream test tests/
dreamer test
dream test --filter addition tests/
```

`@test` is for a top-level `fun name(): void` or `async fun name(): void`, never `main`. Test files must not declare `main`. Put tests in a test project (`type = "test"`) or a package's `tests/` folder; see [Test a project](../../tooling/testing.md).

`@timeout(ms)` sets a limit for one test. There is no default limit.

```dream
@test
@timeout(500)
fun parses_quickly(): void {
    Assert.eq(parse("1 + 2"), 3);
}
```

`dream test` builds one runner that calls `Test.run` for each test. You can also call it from your own `main`:

```dream
fun main() {
    Test.run(
        "addition works",
        () => {
            Assert.eq(2 + 2, 4);
        }
    );
}
```

`Test.run(name, body)` prints `RUN <name>` before the body and `PASS <name>` after it. When the body leaves more live objects than it found, it prints `LEAK <name> live=+N` first. `Test.run_with_timeout(name, ms, body)` also prints the limit (`RUN <name> timeout=<ms>`); `dream test` enforces it.

| Call | Meaning |
| --- | --- |
| `Assert.eq` / `Assert.ne` | equal / not equal |
| `Assert.is_true` / `is_false` | bool |
| `Assert.approx(a, b, eps)` | floats |
| `Assert.eq_str` | strings |
| `Assert.fail(message)` | fail now |
