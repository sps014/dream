---
title: Testing
summary: "Write `@test` functions with assertions."
icon: material/check-decagram-outline
api: reference/api/system-testing.md
---

# Testing


Mark a function with `@test`, check results with assertions, and run `dreamer test` or `dream test`. A failed assertion stops the test and reports the problem.

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

`@test` is a top-level `fun name(): void` or `async fun name(): void`, never `main`, and the file must not declare `main`. Put tests in a test project (`type = "test"`) or a package's `tests/` folder ([Test a project](../../tooling/testing.md)).

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

`Test.run(name, body)` prints `RUN <name>` before the body and `PASS <name>` after it. Extra live objects print `LEAK <name> live=+N` first, and `Test.run_with_timeout` prints `timeout=<ms>`, which `dream test` enforces.

| Call | Meaning |
| --- | --- |
| `Assert.eq` / `Assert.ne` | equal / not equal |
| `Assert.is_true` / `is_false` | bool |
| `Assert.approx(a, b, eps)` | floats |
| `Assert.eq` | strings |
| `Assert.fail(message)` | fail now |
