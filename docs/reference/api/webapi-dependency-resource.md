# Dependency Resource

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-server/index.md) for runnable examples, lifecycle, and error behavior.

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

## `DependencyOverrides`

Generated, type-checked dependency replacement selection for one application.

```dream
public class DependencyOverrides
```

## `constructor`

Creates a `DependencyOverrides` with `providers`.

```dream
public constructor(providers: Map<string, string>)
```
