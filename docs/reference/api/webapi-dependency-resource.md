# DependencyResource

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `DependencyResource`

A dependency value and its asynchronous release operation.

```dream
public class DependencyResource<T>
```

## `value`

Value carried by this entry.

```dream
public value: T
```

## `cleanup`

Asynchronous cleanup action, given the cancellation token.

```dream
public cleanup: fun(CancellationToken): Future<void>
```

## `constructor`

Creates a `DependencyResource` with `value`, `cleanup`.

```dream
public constructor(value: T, cleanup: fun(CancellationToken): Future<void>)
```
