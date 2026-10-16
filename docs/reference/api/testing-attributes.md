# system.testing: Attribute declarations

Import `system.testing` to use these declarations.

## `TestAttribute`

Marks a top-level `fun name(): void` as a unit test discovered by `dream test` / `dreamer test`.

```dream
public struct TestAttribute
```

## `TimeoutAttribute`

Fails a `@test` function that runs longer than this many milliseconds. Tests have no timeout unless they declare one.

```dream
public struct TimeoutAttribute
```

## `milliseconds`

Timeout or interval in milliseconds.

```dream
public milliseconds: int
```
