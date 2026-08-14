# Encoding

**Import:** `import system.encoding;`

Convert between `string`, `byte[]`, hex, and Base64. Hex and Base64 **decode** return `Result` on bad input.

```dream
import system;
import system.encoding;

fun main() {
    let bytes = Encoding.utf8_encode("hi");
    System.println(Encoding.hex_encode(bytes));      // 6869
    System.println(Encoding.utf8_decode(bytes));     // hi
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

Use this before [crypto](crypto.md) hashes, which take `byte[]`.
