# TempCapabilities

**Import:** `import system.io;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `TempCapabilities`

Availability of secure OS temporary resources on this target.

```dream
public static class TempCapabilities
```

## `supported`

True on native and Node; browser virtual filesystems do not provide OS-private resources.

```dream
public static get supported(): bool
```
