# HmacSha256

**Import:** `import system.crypto;`

Read the [usage guide](../stdlib/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class HmacSha256`

HMAC-SHA256 keyed digest (32-byte output). Host-backed via `Dream.cryptoHmacSha256`.

```dream
public static class HmacSha256
```

## `sign`

Computes HMAC-SHA256 of `data` under `key`.

```dream
public static fun sign(key: byte[], data: byte[]): byte[]
```
