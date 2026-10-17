# OnCallAttribute

**Import:** `import system.codegen;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `OnCallAttribute`

Runs the generator for call sites of the named functions or methods (`Type.method`).

```dream
public struct OnCallAttribute
```

## `declaration`

Declaration associated with this entry.

```dream
public declaration: DeclarationPath
```

## `additional_declarations`

Additional declarations made available to the generator.

```dream
public additional_declarations: List<DeclarationPath>
```
