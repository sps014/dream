# JsError

**Import:** `import system.js;`

Read the [JavaScript interop guide](../../language/interop/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsError`

A rejected JavaScript promise, retaining the original rejection value.

```dream
public struct JsError
```

## `reason`

The original JavaScript value, which may be an Error, a primitive, null, or undefined.

```dream
public reason: js
```

## `message`

A readable representation of the rejection value.

```dream
public get message(): string
```
