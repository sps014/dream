# KeyCode

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

[All sections](gpu-gpu-key-code.md)

## `NumpadAdd`

```dream
NumpadAdd
```

## `NumpadSubtract`

```dream
NumpadSubtract
```

## `NumpadMultiply`

```dream
NumpadMultiply
```

## `NumpadDivide`

```dream
NumpadDivide
```

## `NumpadDecimal`

```dream
NumpadDecimal
```

## `NumpadEnter`

```dream
NumpadEnter
```

## `key_code_name`

UI Events / host physical code string for a `KeyCode`.

```dream
public fun key_code_name(code: KeyCode): string
```

## `key_code_from_name`

Map a host physical code string to `KeyCode` (`Unknown` if unrecognized).

```dream
public fun key_code_from_name(name: string): KeyCode
```
