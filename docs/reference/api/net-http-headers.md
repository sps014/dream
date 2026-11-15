# HttpHeaders

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HttpHeaders`

Case-insensitive header names with ordered repeated values. Validation occurs when sending.

```dream
public class HttpHeaders
```

## `constructor`

Creates an empty header collection.

```dream
public constructor()
```

## `length`

Number of stored header pairs, including repeated names.

```dream
public get length(): int
```

## `append`

Appends a value without replacing existing values for the same name.

```dream
public fun append(name: string, value: string): void
```

## `set`

Replaces all existing values for `name` with one value at the end of the collection.

```dream
public fun set(name: string, value: string): void
```

## `get`

Returns the first matching value, or `None` when absent.

```dream
public fun get(name: string): Option<string>
```

## `get_all`

Copies every matching value in stored order; JS Fetch may already have combined repeated headers.

```dream
public fun get_all(name: string): string[]
```

## `remove`

Removes all values with the given case-insensitive name.

```dream
public fun remove(name: string): void
```

## `to_json`

Serializes header pairs in order, preserving repeated names.

```dream
public fun to_json(): JsonValue
```
