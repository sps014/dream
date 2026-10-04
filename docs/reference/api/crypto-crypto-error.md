# CryptoError

**Import:** `import system.crypto;`

Read the [usage guide](../stdlib/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class CryptoError : Error`

Cryptographic operation failure implementing `Error` (currently `AesGcm`/`AesGcmKey`).

```dream
public class CryptoError : Error
```

## `constructor`

Creates an error with a machine code and message.

```dream
public constructor(code: string, message: string)
```

## `message`

Human-readable description.

```dream
public fun message(): string
```

## `code`

Stable machine code (`EINVAL_KEY`, `EINVAL_NONCE`, `EDECRYPT`, …).

```dream
public fun code(): string
```

## `invalid_key`

Key material has the wrong length or shape.

```dream
public static fun invalid_key(message: string): CryptoError
```

## `invalid_nonce`

Nonce/IV has the wrong length.

```dream
public static fun invalid_nonce(message: string): CryptoError
```

## `decrypt_failed`

Decryption failed: authentication tag mismatch, truncated ciphertext, or tampering.

```dream
public static fun decrypt_failed(message: string): CryptoError
```
