# TestFixture

**Import:** `import system.testing;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TestFixture`

Owns one fixture and runs its cleanup on explicit close, scope exit, or async-frame cancellation.

```dream
public class TestFixture<T, E>
```

## `constructor`

Registers deterministic cleanup before the fixture is exposed to test code.

```dream
public constructor(value: T, cleanup: fun(T): Result<bool, E>)
```

## `resource`

Borrows the fixture value while this owner remains alive.

```dream
public get resource(): T
```

## `close`

Executes cleanup once and returns the same outcome to repeated callers.

```dream
public fun close(): Result<bool, E>
```
