# Assert

**Import:** `import system.testing;`

Read the [usage guide](../stdlib/testing.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Assert`

Assertion helpers for `system.testing`. On failure, prints a message and exits the process with a non-zero status (no exceptions/panics — assertions are meant to run in ordinary scripts). Exit-based failures require a real process model (`@native` / `@node`); not available on `@web`.

```dream
public static class Assert
```

## `fail`

Fails the current test run with `message`: prints it and terminates the process with a non-zero exit code. There is no exception mechanism to unwind through, so `Assert.*` failures are unrecoverable by design (matching `System.panic`).

```dream
public static fun fail(message: string): void
```

## `eq`

Asserts that `actual == expected`.

```dream
public static fun eq<T>(actual: T, expected: T): void
```

## `ne`

Asserts that `actual != expected`.

```dream
public static fun ne<T>(actual: T, expected: T): void
```

## `is_true`

Asserts that `value` is `true`.

```dream
public static fun is_true(value: bool): void
```

## `is_false`

Asserts that `value` is `false`.

```dream
public static fun is_false(value: bool): void
```

## `approx`

Asserts that two doubles are within `epsilon` of each other.

```dream
public static fun approx(actual: double, expected: double, epsilon: double): void
```

## `eq_str`

Asserts that two strings have identical contents.

```dream
public static fun eq_str(actual: string, expected: string): void
```
