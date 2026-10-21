# OnAttributeAttribute

**Import:** `import system.codegen;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OnAttributeAttribute`

Runs the generator when a declaration carries one of the named `@attribute` types.

```dream
public struct OnAttributeAttribute
```

## `declaration`

Attribute type whose uses select declarations for the generator.

```dream
public declaration: DeclarationPath
```

## `additional_declarations`

Additional declarations made available to the generator.

```dream
public additional_declarations: List<DeclarationPath>
```
