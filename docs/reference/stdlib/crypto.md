# Crypto

**Import:** `import system.crypto;`

Host-backed hashes, a secure RNG, and AES-256-GCM. APIs return raw `byte[]` — format with [encoding](encoding.md). Not TLS.

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
| Browser | Web Crypto | compile error (`@native` / `@node` only) |

| Call | Meaning |
| --- | --- |
| `Sha256.hash(bytes)` | 32-byte digest |
| `Sha512.hash(bytes)` | 64-byte digest |
| `HmacSha256.sign(key, data)` | HMAC |
| `SecureRandom.bytes(n)` | CSPRNG bytes |
| `AesGcm` / `AesGcmKey` | authenticated encryption (native/Node) |

For games and tests, use [Random](random.md) instead of `SecureRandom`.
