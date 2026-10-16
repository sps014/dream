# Dependency Resource

**Import:** `import system.webapi;`

See the [Web APIs guide](../../web-apis/index.md) for runnable examples, lifecycle, and error behavior.

## `public class DependencyResource<T>`

A dependency value and its asynchronous release operation.

```dream
public class DependencyResource<T>
```

## `public value`

```dream
public value: T
```

## `public cleanup`

```dream
public cleanup: fun(CancellationToken): Future<void>
```

## `public constructor`

```dream
public constructor(value: T, cleanup: fun(CancellationToken): Future<void>)
```

## `public class DependencyOverrides`

Generated, type-checked dependency replacement selection for one application.

```dream
public class DependencyOverrides
```

## `public constructor`

```dream
public constructor(providers: Map<string, string>)
```
