# Promise

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Promise`

Promise utilities for async tasks.

```dream
public static class Promise
```

## `all`

Waits for all promises in the array to resolve.

```dream
public static async fun all<T>(borrow promises: Future<T>[], token: Option<CancellationToken> = Option.None): T[]
```

## `any`

Waits for any promise in the array to resolve.

```dream
public static async fun any<T>(borrow promises: Future<T>[], token: Option<CancellationToken> = Option.None): T
```

## `race`

Returns the first promise to resolve or reject.

```dream
public static async fun race<T>(borrow promises: Future<T>[], token: Option<CancellationToken> = Option.None): T
```
