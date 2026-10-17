# UrlQuery

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `UrlQuery`

Ordered form-decoded URL query parameters. Mutations use form encoding (`+` for space).

```dream
public class UrlQuery
```

## `length`

Number of key/value pairs, including repeated keys and empty values.

```dream
public get length(): int
```

## `append`

Appends a decoded key/value pair after existing parameters.

```dream
public fun append(key: string, value: string): void
```

## `get`

Returns the first decoded value for `key`, or `None` if the key is absent.

```dream
public fun get(key: string): Option<string>
```

## `get_all`

Copies all decoded values for `key` in their original order.

```dream
public fun get_all(key: string): string[]
```

## `set`

Replaces the first occurrence in place and removes later duplicates; appends if absent.

```dream
public fun set(key: string, value: string): void
```

## `remove`

Removes every occurrence of `key`, preserving the order of remaining pairs.

```dream
public fun remove(key: string): void
```

## `clear`

Removes all parameters and the query delimiter from the URL.

```dream
public fun clear(): void
```
