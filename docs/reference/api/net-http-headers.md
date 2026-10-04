# HttpHeaders

**Import:** `import system.net;`

Read the [usage guide](../stdlib/http.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class HttpHeaders : Collection<KeyValuePair<string, string>>`

Typed HTTP header map (replaces JSON-string headers).

```dream
public class HttpHeaders : Collection<KeyValuePair<string, string>>
```

## `constructor`

Creates an empty header map.

```dream
public constructor()
```

## `set`

Sets header `name` to `value`, overwriting any existing entry (case-insensitive name match).

```dream
public fun set(name: string, value: string): void
```

## `get`

Value for `name`, or `None` when absent (case-insensitive).

```dream
public fun get(name: string): Option<string>
```

## `contains`

True when a header named `name` is present (case-insensitive).

```dream
public fun contains(name: string): bool
```

## `remove`

Removes `name` if present; returns whether a header was removed.

```dream
public fun remove(name: string): bool
```

## `length`

Number of stored header pairs.

```dream
public get length(): int
```

## `iterator`

Iterates `(name, value)` pairs in insertion order without snapshotting into a new list.

```dream
public fun iterator(): HttpHeadersIterator
```

## `to_wire`

Wire format for the host bridge (JSON object string).

```dream
public fun to_wire(): string
```

## `from_wire`

Rebuilds headers from the host bridge JSON object string.

```dream
public static fun from_wire(borrow text: string): HttpHeaders
```

## `add_all`

Copies every pair from `other` (later names overwrite).

```dream
public fun add_all(borrow other: HttpHeaders): void
```
