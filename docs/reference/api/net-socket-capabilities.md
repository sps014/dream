# SocketCapabilities

**Import:** `import system.net;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `SocketCapabilities`

Availability of raw sockets independently of the host's HTTP support.

```dream
public static class SocketCapabilities
```

## `supported`

True on native and Node; browsers return EUNSUPPORTED for raw socket requests.

```dream
public static get supported(): bool
```
