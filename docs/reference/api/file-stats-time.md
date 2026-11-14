# FileStats times

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `modified_utc`

`modified` as a UTC `DateTime`.

```dream
public fun modified_utc(): DateTime
```

## `changed_utc`

`changed` as a UTC `DateTime` (inode change time on Unix, not birth time).

```dream
public fun changed_utc(): DateTime
```
