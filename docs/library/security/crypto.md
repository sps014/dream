---
title: Crypto
summary: "Secure random values, hashes, and AES-GCM encryption."
icon: material/key-chain-variant
api: reference/api/system-crypto.md
---

# Crypto


These APIs hash data, draw secure random bytes, and encrypt with authentication. They return `byte[]` ([Encoding](../text/encoding.md) turns that into text); check the table before encrypting in a browser.

**Import:** `import system.crypto;`

SHA-256, SHA-512, and HMAC-SHA256 match on every target, including the browser. The secure RNG uses the operating system on native and Node, and Web Crypto in the browser; these calls are not TLS.

```dream
import system;
import system.crypto;
import system.encoding;

fun main() {
    let msg = Encoding.utf8_encode("hello");
    System.println(Encoding.hex_encode(Sha256.hash(msg)));
}
```

| Runtime | Digests / CSPRNG | AES-GCM |
| --- | --- | --- |
| Native / Node | yes | yes |
| Browser | Web Crypto | compile error (native and Node.js only) |

| Call | Meaning |
| --- | --- |
| `Sha256.hash(bytes)` | 32-byte digest |
| `Sha512.hash(bytes)` | 64-byte digest |
| `HmacSha256.sign(key, data)` | HMAC |
| `SecureRandom.bytes(n)` | CSPRNG bytes |
| `AesGcm` / `AesGcmKey` | authenticated encryption (native/Node) |

```dream
let key = AesGcmKey.generate().unwrap();
let nonce = AesGcm.generate_nonce().unwrap();
let plain = Encoding.utf8_encode("secret");
let aad = Encoding.utf8_encode("");
let cipher = (AesGcm.encrypt(key, nonce, plain, aad)).unwrap();
let back = (AesGcm.decrypt(key, nonce, cipher, aad)).unwrap();
System.println(Encoding.utf8_decode(back));
```

For games and tests, use [Random](../system/random.md) instead of `SecureRandom`.
