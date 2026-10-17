# Reader

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Reader`

Byte input with partial reads and optional positioning; operations report I/O failures.

```dream
public interface Reader
```

## `read`

Reads at most `max_bytes` bytes; empty success means EOF for a positive size. A zero size consumes nothing; negative sizes and closed resources return errors.

```dream
unique fun read(max_bytes: int): Result<byte[], IoError>
```

## `seek`

Moves to an absolute byte position; unsupported readers return `ENOTSUP`. A failed seek preserves the previous logical position.

```dream
public unique fun seek(position: long): Result<bool, IoError>
```

## `tell`

Returns the logical byte position, or `ENOTSUP` when positioning is unavailable.

```dream
public fun tell(): Result<long, IoError>
```

## `read_exact`

Collects exactly `count` bytes, retrying short reads; premature EOF returns `EEOF`. Errors may occur after input has already been consumed; negative counts are invalid.

```dream
public fun read_exact(count: int): Result<byte[], IoError>
```
