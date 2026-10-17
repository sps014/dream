# Utf8Decoder

**Import:** `import system.text;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `Utf8Decoder`

Incremental UTF-8 decoding that retains incomplete sequences between chunks and preserves a leading byte-order mark as text.

```dream
public class Utf8Decoder
```

## `constructor`

Creates an active decoder with strict validation by default.

```dream
public constructor(policy: InvalidEncoding = InvalidEncoding.Strict)
```

## `decode_chunk`

Decodes complete scalars and retains a trailing incomplete sequence for the next chunk. Malformed input follows the selected policy; calls after failure or finish return errors.

```dream
public fun decode_chunk(bytes: byte[]): Result<string, ParseError>
```

## `finish`

Ends input, rejecting a truncated sequence in strict mode or replacing it with U+FFFD. Returns any trailing text; all subsequent calls return an inactive-decoder error.

```dream
public fun finish(): Result<string, ParseError>
```
