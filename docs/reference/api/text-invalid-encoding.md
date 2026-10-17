# InvalidEncoding

**Import:** `import system.text;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `InvalidEncoding`

Policy for malformed or truncated UTF-8 input.

```dream
public enum InvalidEncoding
```

## `Strict`

Returns a parse error and permanently fails the decoder on invalid input.

```dream
Strict
```

## `Replace`

Emits U+FFFD replacement characters for invalid input and continues decoding.

```dream
Replace
```
