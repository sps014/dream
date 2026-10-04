# Test

**Import:** `import system.testing;`

Read the [usage guide](../stdlib/testing.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Test`

Minimal test runner: prints PASS/FAIL per named case, everything else is `Assert.*`.

```dream
public static class Test
```

## `run`

Runs `body`, printing `PASS <name>` on success. Assertion failures inside `body` already print their own message and exit the process (see `Assert.fail`), so a case reaching the print below never failed.

```dream
public static fun run(name: string, body: fun(): void): void
```
