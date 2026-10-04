# AesGcm

**Import:** `import system.crypto;`

Read the [usage guide](../stdlib/crypto.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class AesGcm`

AES-256-GCM authenticated encryption (confidentiality + integrity). Host-backed via the `aes-gcm` crate natively and Web Crypto / `node:crypto` in the browser/Node hosts.

```dream
public static class AesGcm
```

## `nonce_size`

Standard 96-bit (12-byte) GCM nonce size.

```dream
public static get nonce_size(): int
```

## `tag_size`

128-bit (16-byte) authentication tag appended to every ciphertext.

```dream
public static get tag_size(): int
```

## `generate_nonce`

Generates a fresh random 12-byte nonce. Never reuse a nonce with the same key.

```dream
public static fun generate_nonce(): byte[]
```

## `encrypt`

Encrypts `plaintext` under `key`/`nonce`, authenticating `aad` (additional authenticated data, not encrypted but tamper-checked) alongside it. Returns ciphertext with a 16-byte authentication tag appended. `nonce` must be exactly `nonce_size` bytes and must never be reused with the same key.

```dream
public static fun encrypt(key: AesGcmKey, nonce: byte[], plaintext: byte[], aad: byte[]): Result<byte[], CryptoError>
```

## `decrypt`

Decrypts and authenticates `ciphertext` (as produced by `encrypt`, tag included) under `key`/`nonce`/`aad`. Returns an error if the nonce length is wrong or authentication fails (tampered ciphertext, wrong key/nonce/aad, or truncated input).

```dream
public static fun decrypt(key: AesGcmKey, nonce: byte[], ciphertext: byte[], aad: byte[]): Result<byte[], CryptoError>
```
