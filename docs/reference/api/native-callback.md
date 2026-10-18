# NativeCallback

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `NativeCallback`

A Dream `fun` (capturing or not) handed to a `@c` extern as a `(fn, void* user_data)` pair. C may call it for as long as this object is alive: keep it in a field while C holds the pointer. Calls from threads Dream did not start trap.

```dream
public class NativeCallback<F>
```

## `constructor`

Creates a `NativeCallback` with `f`.

```dream
public constructor(f: F)
```
