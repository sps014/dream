# Testing

**Package:** `system.testing` — `import system.testing;`

Minimal assertion + test-runner helpers for ordinary Dream scripts (not a full framework — there's no discovery or reporting beyond stdout). Assertion failures are unrecoverable: Dream has no exceptions, so `Assert.*` prints the failure and exits the process via `System.exit(1)`.

```dream
import system;
import system.testing;
```

#### `Test.run(name: string, body: fun(): void): void`

Runs `body` and prints `PASS <name>` if it returns normally. If an assertion inside `body` fails, the process exits before printing — the failure message from `Assert.fail` is the last line of output.

```dream
Test.run("addition works", () => {
    Assert.eq(2 + 2, 4);
});
```

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
