# String UTF-8

No import is needed.

Read the [usage guide](../../library/text/encoding.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `from_utf8`

Interprets `bytes` as UTF-8. Invalid sequences become U+FFFD, and a leading BOM is kept.

```dream
public static extern fun from_utf8(borrow bytes: byte[]): string
```

## `to_utf8`

Returns the UTF-8 encoding of this string. A surrogate pair becomes one code point; a lone surrogate is encoded as that code unit.

```dream
public fun to_utf8(): byte[]
```
