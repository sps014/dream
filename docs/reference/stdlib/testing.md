# Testing

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

`@test` is only for a top-level `fun name(): void` (not `async`, not `main`). Test files must not declare `main`. Prefer a project `tests/` folder.

Without discovery, call `Test.run` from `main`:

```dream
fun main() {
    Test.run("addition works", () => {
        Assert.eq(2 + 2, 4);
    });
}
```

| Call | Meaning |
| --- | --- |
| `Assert.eq` / `Assert.ne` | equal / not equal |
| `Assert.is_true` / `is_false` | bool |
| `Assert.approx(a, b, eps)` | floats |
| `Assert.eq_str` | strings |
| `Assert.fail(message)` | fail now |
