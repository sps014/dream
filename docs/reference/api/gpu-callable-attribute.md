# CallableAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `CallableAttribute`

Marks a callable shader, a function other ray tracing shaders invoke by index with `call_shader`. Its only parameter is `ref` to the data passed by the caller.

```dream
public struct CallableAttribute
```
