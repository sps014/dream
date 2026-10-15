# system.testing: Attribute declarations

Import `system.testing` to use these declarations.

```dream
public struct TestAttribute
```

Marks a top-level `fun name(): void` as a unit test discovered by `dream test` / `dreamer test`.

```dream
public struct TimeoutAttribute
```

Fails a `@test` function that runs longer than this many milliseconds. Tests have no timeout unless they declare one.

```dream
public milliseconds: int
```
