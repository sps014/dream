# Secure Random

**Import:** `import system.crypto;`

## `SecureRandom`

Cryptographically secure bytes with explicit entropy-source failures.

```dream
public static class SecureRandom
```

## `bytes`

Returns exactly n secure bytes, or EINVAL/EENTROPY without exposing partial output.

```dream
public static fun bytes(n: int): Result<byte[], CryptoError>
```

## `fill`

Fills the destination only after obtaining all requested entropy; failure preserves it.

```dream
public static fun fill(dest: byte[]): Result<bool, CryptoError>
```
