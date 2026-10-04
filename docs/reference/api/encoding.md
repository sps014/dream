# Encoding

**Import:** `import system.encoding;`

Read the [usage guide](../stdlib/encoding.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `static class Encoding`

UTF-8 / hex / Base64 encode and decode helpers.

```dream
public static class Encoding
```

## `utf8_encode`

Copies the UTF-8 encoding of `text` into a new `byte[]`.

```dream
public static fun utf8_encode(text: string): byte[]
```

## `utf8_decode`

Interprets `bytes` as UTF-8 and builds a Dream string.

```dream
public static fun utf8_decode(bytes: byte[]): string
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

## `url_encode`

Percent-encodes `text` as UTF-8 bytes (RFC 3986 unreserved set left as-is).

```dream
public static fun url_encode(text: string): string
```

## `url_decode`

Decodes a percent-encoded URL component.

```dream
public static fun url_decode(text: string): Result<string, ParseError>
```
