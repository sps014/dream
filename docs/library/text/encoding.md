---
title: Encoding
summary: "Convert between text, bytes, UTF-8, hex, and Base64."
icon: material/swap-vertical
api: reference/api/system-encoding.md
---

# Encoding


Encoding converts text and bytes between representations, including UTF-8, hexadecimal, and Base64. Decoding hexadecimal or Base64 returns a Result so you can handle invalid input.

**Import:** `import system.encoding;`

Convert between `string`, `byte[]`, hex, and Base64. Hex and Base64 **decode** return `Result` on bad input.

```dream
import system;
import system.encoding;

fun main() {
    let bytes = Encoding.utf8_encode("hi");
    System.println(Encoding.hex_encode(bytes)); // 6869
    System.println(Encoding.utf8_decode(bytes)); // hi
}
```

| Call | Meaning |
| --- | --- |
| `Encoding.utf8_encode(text)` | string → UTF-8 bytes |
| `Encoding.utf8_decode(bytes)` | UTF-8 bytes → string |
| `Encoding.hex_encode(bytes)` | lowercase hex |
| `Encoding.hex_decode(text)` | `Result<byte[], ParseError>` |
| `Encoding.base64_encode(bytes)` | Base64 |
| `Encoding.base64_decode(text)` | `Result<byte[], ParseError>` |
| `Encoding.base64url_encode` / `decode` | URL-safe Base64 |
| `Encoding.uri_encode_component` / `uri_decode_component` | URI-component percent-encoding |
| `Encoding.form_encode_component` / `form_decode_component` | form encoding (`+` for spaces) |

Use this before [crypto](../security/crypto.md) hashes, which take `byte[]`.

## Incremental UTF-8

Import `system.text` for strict incremental decoding. The decoder carries partial
sequences between chunks and rejects malformed, overlong, surrogate, and
out-of-range encodings. Call `finish()` to detect a truncated final sequence.

```dream
import system.text;

fun main(): Result<bool, ParseError> {
    let decoder = Utf8Decoder();
    let first = decoder.decode_chunk([(byte)195])?;
    let second = decoder.decode_chunk([(byte)169])?;
    let tail = decoder.finish()?;
    return Result.Ok(first + second + tail == "é");
}
```

Use `InvalidEncoding.Replace` to emit replacement characters for invalid input.
A decoder cannot be reused after `finish()` or a strict decoding error.

URI-component decoding preserves `+`. Form decoding converts `+` to a space;
both validate percent escapes and decoded UTF-8, including raw Unicode input.
