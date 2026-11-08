# JsTypeAttribute

**Import:** `import system.js;`

Read the [JavaScript interop guide](../../language/interop/index.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `JsTypeAttribute`

Identifies the originating namespace of a JavaScript binding. The struct must contain one owned `js` field and a constructor accepting that handle. A static class may instead identify a module's exported members.

```dream
public struct JsTypeAttribute
```

## `namespace`

JavaScript module or standard-library namespace shown in editor type information.

```dream
public namespace: string
```
