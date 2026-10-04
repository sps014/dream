# Sha512

**Import:** `import system.crypto;`

Read the [usage guide](../stdlib/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Sha512`

SHA-512 digest (64-byte output). Host-backed via `Dream.cryptoSha512`.

```dream
public static class Sha512
```

## `hash`

Digests `data` and returns the 64-byte hash.

```dream
public static fun hash(data: byte[]): byte[]
```
