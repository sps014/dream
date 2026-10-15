# Aes Gcm Key

**Import:** `import system.crypto;`

## `AesGcmKey`

A 256-bit `AesGcm` key. Wraps raw bytes so callers can't accidentally pass the wrong-sized buffer where a key is expected.

```dream
public class AesGcmKey
```

## `generate`

Generates a fresh random 256-bit key from the OS CSPRNG.

```dream
public static fun generate(): Result<AesGcmKey, CryptoError>
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
