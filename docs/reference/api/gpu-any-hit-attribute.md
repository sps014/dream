# AnyHitAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside `@compute`, `@vertex`, `@fragment`, inferred helpers and ray tracing shader functions; using them elsewhere is a compile error.

## `AnyHitAttribute`

Marks an any-hit shader, which runs for every candidate intersection along a ray, in no particular order. Use it for alpha testing: call `ignore_hit()` to skip a candidate or `accept_hit_and_end_search()` to stop at it. Parameters match `@closest_hit`.

```dream
public struct AnyHitAttribute
```
