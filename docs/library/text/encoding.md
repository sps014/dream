---
title: Encoding
summary: "Convert between bytes, hexadecimal, and Base64."
icon: material/swap-vertical
api: reference/api/system-encoding.md
---

# Encoding


Encoding converts bytes to hexadecimal and Base64. Decoding returns a Result so you can handle invalid input. Convert text with `to_utf8` and `string.from_utf8`, which are always available; invalid UTF-8 becomes U+FFFD.

**Import:** `import system.encoding;`

```dream
import system;
import system.encoding;

fun main() {
    let bytes = "hi".to_utf8();
    System.println(Encoding.hex_encode(bytes)); // 6869
    System.println(string.from_utf8(bytes)); // hi
}
```

| Call | Meaning |
| --- | --- |
| `text.to_utf8()` | string → UTF-8 bytes |
| `string.from_utf8(bytes)` | UTF-8 bytes → string; invalid sequences become U+FFFD |
| `Encoding.hex_encode(bytes)` | lowercase hex |
| `Encoding.hex_decode(text)` | `Result<byte[], ParseError>` |
| `Encoding.base64_encode(bytes)` | Base64 |
| `Encoding.base64_decode(text)` | `Result<byte[], ParseError>` |
| `Encoding.base64url_encode` / `decode` | URL-safe Base64 |
| `Encoding.uri_encode_component` / `uri_decode_component` | URI-component percent-encoding |
| `Encoding.form_encode_component` / `form_decode_component` | form encoding (`+` for spaces) |

Use this before [crypto](../security/crypto.md) hashes, which take `byte[]`.

## Incremental UTF-8

Import `system.text` for strict incremental decoding. Partial sequences carry across chunks, and malformed, overlong, surrogate, or out-of-range input is rejected; call `finish()` to catch a truncated ending.

```dream
import system.text;

fun main(): Result<Unit, ParseError> {
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
