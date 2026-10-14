# Encoding

**Import:** `import system.encoding;`

## `public static class Encoding`

UTF-8 / hex / Base64 encode and decode helpers.

```dream
public static class Encoding
```

## `public static fun utf8_encode`

Copies the UTF-8 encoding of `text` into a new `byte[]`.

```dream
public static fun utf8_encode(text: string): byte[]
```

## `public static fun utf8_decode`

Interprets `bytes` as UTF-8 and builds a Dream string.

```dream
public static fun utf8_decode(bytes: byte[]): string
```

## `public static fun hex_encode`

Encodes `bytes` as lowercase hexadecimal text.

```dream
public static fun hex_encode(bytes: byte[]): string
```

## `public static fun hex_decode`

Decodes hexadecimal text into bytes; rejects odd length and non-hex digits.

```dream
public static fun hex_decode(text: string): Result<byte[], ParseError>
```

## `public static fun base64_encode`

Encodes `bytes` as standard Base64 (with `=` padding).

```dream
public static fun base64_encode(bytes: byte[]): string
```

## `public static fun base64_decode`

Decodes Base64 text (whitespace ignored); rejects invalid alphabet characters.

```dream
public static fun base64_decode(text: string): Result<byte[], ParseError>
```

## `public static fun base64url_encode`

Encodes `bytes` as Base64url without padding.

```dream
public static fun base64url_encode(bytes: byte[]): string
```

## `public static fun base64url_decode`

Decodes Base64url text (`-`/`_`, padding optional).

```dream
public static fun base64url_decode(text: string): Result<byte[], ParseError>
```

## `public static fun uri_encode_component`

Percent-encodes `text` as UTF-8 bytes (RFC 3986 unreserved set left as-is).

```dream
public static fun uri_encode_component(text: string): string
```

## `public static fun uri_decode_component`

Decodes percent escapes as strict UTF-8, preserving literal `+`. Malformed escapes or invalid UTF-8 return a parse error.

```dream
public static fun uri_decode_component(text: string): Result<string, ParseError>
```

## `public static fun form_encode_component`

Encodes a form component as UTF-8, using `+` for spaces and percent escapes except for ASCII letters, digits, `*`, `-`, `.`, and `_`.

```dream
public static fun form_encode_component(text: string): string
```

## `public static fun form_decode_component`

Decodes a form component, interpreting `+` as space and validating escaped UTF-8. Malformed escapes or invalid UTF-8 return a parse error.

```dream
public static fun form_decode_component(text: string): Result<string, ParseError>
```
