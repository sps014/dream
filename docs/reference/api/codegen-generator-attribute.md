# GeneratorAttribute

**Import:** `import system.codegen;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `GeneratorAttribute`

Marks `fun name(ctx: GenContext)` as a source generator (`system.codegen`). The function name is the generator name.

```dream
public struct GeneratorAttribute
```
