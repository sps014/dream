# DependencyOverrideAttribute

**Import:** `import system.webapi;`

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `DependencyOverrideAttribute`

Replaces a provider for applications explicitly installing the containing class's dependencies().

```dream
public struct DependencyOverrideAttribute
```

## `provider`

Dependency function to evaluate for this parameter or replace for this application.

```dream
public provider: DeclarationPath
```
