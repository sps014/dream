# BuiltinAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `BuiltinAttribute`

Connects a field or parameter of a shader's input or output struct to a value the GPU itself produces or consumes.  `@builtin("position")` on a vertex shader output is the clip-space position; on a fragment shader input it is the pixel's window position. Other names are `"front_facing"`, `"sample_index"` and `"frag_depth"`. Unknown names are build errors.

```dream
public struct BuiltinAttribute
```

## `name`

The built-in value's name, such as `"position"`.

```dream
public name: string
```
