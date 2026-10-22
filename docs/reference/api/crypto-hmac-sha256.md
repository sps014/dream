# HmacSha256

**Import:** `import system.crypto;`

Read the [usage guide](../../library/security/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `HmacSha256`

HMAC-SHA256 keyed message authentication (RFC 2104) with a 32-byte result.

```dream
public static class HmacSha256
```

## `sign`

Computes HMAC-SHA256 of `data` under `key`. Keys of any length are accepted; keys longer than 64 bytes are hashed first, as RFC 2104 specifies.

```dream
public static fun sign(borrow key: byte[], borrow data: byte[]): byte[]
```
