# AesGcmKey

**Import:** `import system.crypto;`

Read the [usage guide](../stdlib/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class AesGcmKey`

A 256-bit `AesGcm` key. Wraps raw bytes so callers can't accidentally pass the wrong-sized buffer where a key is expected.

```dream
public class AesGcmKey
```

## `generate`

Generates a fresh random 256-bit key from the OS CSPRNG.

```dream
public static fun generate(): AesGcmKey
```

## `from_bytes`

Wraps exactly 32 raw key bytes, or an error when the length is wrong.

```dream
public static fun from_bytes(bytes: byte[]): Result<AesGcmKey, CryptoError>
```

## `to_bytes`

The raw 32 key bytes.

```dream
public fun to_bytes(): byte[]
```
