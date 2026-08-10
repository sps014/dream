# Testing

**Package:** `system.testing` — `import system.testing;`

Mark tests with `@test`, then run them with `dream test` or `dreamer test`. Assertion failures print a message and exit via `System.exit(1)` (fail-fast; Dream has no exceptions).

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
dream test tests/                 # every *.dream under tests/
dreamer test                      # install deps, then dream test tests/
dream test --filter addition tests/
```

`@test` applies only to top-level `fun name(): void` (no parameters, not `async`, not `main`). Test files must not declare `main` — the runner synthesizes one. Prefer a project `tests/` directory; `dreamer test` requires it.

You can still call `Test.run` from an ordinary `main` when you want a single script without discovery:

```dream
fun main(): void {
    Test.run("addition works", () => {
        Assert.eq(2 + 2, 4);
    });
}
```

#### `Test.run(name: string, body: fun(): void): void`

Runs `body` and prints `PASS <name>` if it returns normally. If an assertion inside `body` fails, the process exits before printing — the failure message from `Assert.fail` is the last line of output.

#### `Assert.eq<T>(actual: T, expected: T): void` / `Assert.ne<T>(actual: T, expected: T): void`

Asserts equality/inequality using `==`/`!=` (built-in equality for primitives/strings, `Equatable` for user types that implement it). The failure message includes both operands via `to_string`.

```dream
Assert.eq(1 + 1, 2);
Assert.ne(1, 2);
```

#### `Assert.is_true(value: bool): void` / `Assert.is_false(value: bool): void`

Asserts a boolean condition.

```dream
Assert.is_true(1 < 2);
Assert.is_false(2 < 1);
```

#### `Assert.approx(actual: double, expected: double, epsilon: double): void`

Asserts two doubles are within `epsilon` of each other. Use instead of `eq` for floating-point results.

```dream
Assert.approx(0.1d + 0.2d, 0.3d, 0.0001d);
```

#### `Assert.eq_str(actual: string, expected: string): void`

Asserts string content equality; on failure the message includes both values.

```dream
Assert.eq_str("hello", "hello");
```

#### `Assert.fail(message: string): void`

Fails unconditionally — every other `Assert.*` method delegates to this on failure. Call directly for custom checks.

```dream
if (!some_invariant) {
    Assert.fail("invariant violated");
}
```

`Assert.*` and `System.exit` are `@native` / `@node` only (no process model on the web target).
