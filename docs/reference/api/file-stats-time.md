# FileStats times

**Import:** `import system;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `modified`

`mtime` as a UTC `DateTime`.

```dream
public fun modified(): DateTime
```

## `created`

`ctime` as a UTC `DateTime` (inode change time on Unix, not birth time).

```dream
public fun created(): DateTime
```
