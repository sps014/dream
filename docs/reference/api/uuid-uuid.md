# Uuid

**Import:** `import system.uuid;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Uuid`

A 16-byte UUID value; assignment copies bits without allocation or reference counting.

```dream
public struct Uuid: Equatable<Uuid>, Comparable<Uuid>
```

## `v4`

Generates a version-4 UUID from the secure random source, preserving entropy failures.

```dream
public static fun v4(): Result<Uuid, UuidError>
```

## `from_bytes`

Copies exactly 16 bytes in network order into a UUID value.

```dream
public static fun from_bytes(bytes: byte[]): Result<Uuid, UuidError>
```

## `to_bytes`

Returns an independent 16-byte array in network order.

```dream
public fun to_bytes(): byte[]
```

## `parse`

Accepts exactly 36 ASCII hexadecimal characters with canonical hyphen positions.

```dream
public static fun parse(text: string): Result<Uuid, UuidError>
```

## `is_valid`

Tests canonical UUID syntax without generating entropy.

```dream
public static fun is_valid(text: string): bool
```

## `equals`

Compares all 128 bits, independent of storage identity.

```dream
public fun equals(borrow other: Uuid): bool
```

## `compare`

Orders UUIDs lexicographically by their unsigned network-order bytes.

```dream
public fun compare(other: Uuid): int
```

## `hash_code`

Folds all bits into a stable hash for maps and sets without formatting text.

```dream
public override fun hash_code(): int
```

## `to_string`

Returns canonical lowercase text; formatting is the only string allocation.

```dream
public override fun to_string(): string
```
