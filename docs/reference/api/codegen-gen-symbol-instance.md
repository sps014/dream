# GenSymbolInstance

**Import:** `import system.codegen;`

## `GenSymbolInstance`

Concrete semantic members, including inherited defaults and substituted generic arguments.

```dream
public class GenSymbolInstance
```

## `ty`

Type of this declaration or expression.

```dream
public ty: GenTypeRef
```

## `bases`

Base types declared by this type.

```dream
public bases: List<GenTypeRef>
```

## `fields`

Fields of this type, in declaration order.

```dream
public fields: List<GenField>
```

## `methods`

Methods declared by this type.

```dream
public methods: List<GenFunction>
```

## `variants`

Choices declared by this union or enum, in declaration order.

```dream
public variants: List<GenVariant>
```

## `constructor`

Decodes an instantiated type view exported by semantic analysis.

```dream
public constructor(borrow v: JsonValue)
```
