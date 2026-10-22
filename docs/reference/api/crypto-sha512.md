# Sha512

**Import:** `import system.crypto;`

Read the [usage guide](../../library/security/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Sha512`

SHA-512 message digest (FIPS 180-4). The same input always produces the same 64-byte hash on every target.

```dream
public static class Sha512
```

## `hash`

Digests `data` and returns its 64-byte SHA-512 hash.

```dream
public static fun hash(borrow data: byte[]): byte[]
```
