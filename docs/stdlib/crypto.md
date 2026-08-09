# Crypto

**Package:** `system.crypto` — `import system.crypto;`

Host-backed digests, a cryptographically secure RNG, and AES-256-GCM authenticated encryption. Hex/Base64 live in [`encoding`](encoding.md) — crypto APIs return raw `byte[]`.

## Platform notes

| Runtime | Digests / CSPRNG | AES-GCM |
| --- | --- | --- |
| Native (`dream run`) | OS CSPRNG and native digest libraries | Rust `aes-gcm` crate |
| Node.js | `node:crypto` | `node:crypto` |
| Browser | Web Crypto | Not supported (Web Crypto's AES-GCM API is async-only; the extern ABI here is synchronous) |

Non-goals: TLS, certificates.

```dream
import system;
import system.crypto;
import system.encoding;
```

#### `Sha256.hash(data: byte[]): byte[]`

Computes a 32-byte SHA-256 digest of `data`. Use for content fingerprints and as input to HMAC when a shorter hash is enough.

```dream
let msg = Encoding.utf8_encode("hello");
System.println(Encoding.hex_encode(Sha256.hash(msg)));
```

#### `Sha512.hash(data: byte[]): byte[]`

Computes a 64-byte SHA-512 digest. Prefer when you want a wider hash for long-term integrity or protocol requirements specify SHA-512.

```dream
System.println(Encoding.hex_encode(Sha512.hash(msg)));
```

#### `HmacSha256.sign(key: byte[], data: byte[]): byte[]`

Computes a 32-byte HMAC-SHA256 MAC with `key` over `data`. Use to authenticate messages or API payloads with a shared secret.

```dream
let key = Encoding.utf8_encode("secret");
System.println(Encoding.hex_encode(HmacSha256.sign(key, msg)));
```

#### `SecureRandom.bytes(n: int): byte[]`

Fills a new array with `n` cryptographically secure random bytes (`n <= 0` → empty). Use for tokens, nonces, and IVs — not for gameplay RNG.

```dream
let nonce = SecureRandom.bytes(16);
System.println(Encoding.hex_encode(nonce));
```

#### `SecureRandom.fill(dest: byte[]): void`

Overwrites an existing buffer with secure random bytes in place. Prefer when reusing a fixed-size buffer to avoid allocation.

```dream
let buf = Buffer.alloc<byte>(32);
SecureRandom.fill(buf);
```

For non-cryptographic PRNG, use [`Random`](random.md).

## `AesGcm` / `AesGcmKey`

AES-256-GCM authenticated encryption: confidentiality plus a 16-byte authentication tag that detects tampering. Use for encrypting data at rest or over an already-authenticated channel — not as a replacement for TLS.

#### `AesGcmKey.generate(): AesGcmKey`

Generates a fresh random 256-bit key from the OS CSPRNG.

```dream
let key = AesGcmKey.generate();
```

#### `AesGcmKey.from_bytes(bytes: byte[]): Result<AesGcmKey, CryptoError>`

Wraps exactly 32 raw key bytes (e.g. one derived from a KDF), or returns an error if the length is wrong.

```dream
let key = AesGcmKey.from_bytes(SecureRandom.bytes(32)).unwrap_or(AesGcmKey.generate());
```

#### `AesGcm.generate_nonce(): byte[]`

Generates a fresh random 12-byte nonce. **Never reuse a nonce with the same key** — doing so breaks GCM's confidentiality and integrity guarantees.

```dream
let nonce = AesGcm.generate_nonce();
```

#### `AesGcm.encrypt(key: AesGcmKey, nonce: byte[], plaintext: byte[], aad: byte[]): Result<byte[], CryptoError>`

Encrypts `plaintext`, returning ciphertext with a 16-byte authentication tag appended. `aad` (additional authenticated data) is authenticated but not encrypted or included in the output separately — pass `Buffer.alloc<byte>(0)` when you have none. `nonce` must be exactly 12 bytes.

```dream
let key = AesGcmKey.generate();
let nonce = AesGcm.generate_nonce();
let aad = Buffer.alloc<byte>(0);
let ciphertext = AesGcm.encrypt(key, nonce, Encoding.utf8_encode("secret"), aad).unwrap_or(Buffer.alloc<byte>(0));
```

#### `AesGcm.decrypt(key: AesGcmKey, nonce: byte[], ciphertext: byte[], aad: byte[]): Result<byte[], CryptoError>`

Decrypts and authenticates `ciphertext` (as produced by `encrypt`). Returns `Err(CryptoError)` if the nonce length is wrong or authentication fails — wrong key/nonce/`aad`, or the ciphertext was tampered with or truncated.

```dream
switch (AesGcm.decrypt(key, nonce, ciphertext, aad)) {
    Ok(plaintext) => System.println(Encoding.utf8_decode(plaintext)),
    Err(e) => System.println("decrypt failed: " + e.message()),
}
```
