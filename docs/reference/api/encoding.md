# Encoding

**Import:** `import system.encoding;`

## `Encoding`

Hex and Base64 encode and decode helpers. Convert text with `string.to_utf8` and `string.from_utf8`.

```dream
public static class Encoding
```

## `hex_encode`

Encodes `bytes` as lowercase hexadecimal text.

```dream
public static fun hex_encode(bytes: byte[]): string
```

## `hex_decode`

Decodes hexadecimal text into bytes; rejects odd length and non-hex digits.

```dream
public static fun hex_decode(text: string): Result<byte[], ParseError>
```

## `base64_encode`

Encodes `bytes` as standard Base64 (with `=` padding).

```dream
public static fun base64_encode(bytes: byte[]): string
```

## `base64_decode`

Decodes Base64 text (whitespace ignored); rejects invalid alphabet characters.

```dream
public static fun base64_decode(text: string): Result<byte[], ParseError>
```

## `base64url_encode`

Encodes `bytes` as Base64url without padding.

```dream
public static fun base64url_encode(bytes: byte[]): string
```

## `base64url_decode`

Decodes Base64url text (`-`/`_`, padding optional).

```dream
public static fun base64url_decode(text: string): Result<byte[], ParseError>
```

## `uri_encode_component`

Percent-encodes `text` as UTF-8 bytes (RFC 3986 unreserved set left as-is).

```dream
public static fun uri_encode_component(text: string): string
```

## `uri_decode_component`

Decodes percent escapes as strict UTF-8, preserving literal `+`. Malformed escapes or invalid UTF-8 return a parse error.

```dream
public static fun uri_decode_component(text: string): Result<string, ParseError>
```

## `form_encode_component`

Encodes a form component as UTF-8, using `+` for spaces and percent escapes except for ASCII letters, digits, `*`, `-`, `.`, and `_`.

```dream
public static fun form_encode_component(text: string): string
```

## `form_decode_component`

Decodes a form component, interpreting `+` as space and validating escaped UTF-8. Malformed escapes or invalid UTF-8 return a parse error.

```dream
public static fun form_decode_component(text: string): Result<string, ParseError>
```
