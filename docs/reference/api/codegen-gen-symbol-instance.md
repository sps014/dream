# GenSymbolInstance

**Import:** `import system.codegen;`

## `public class GenSymbolInstance`

Concrete semantic members, including inherited defaults and substituted generic arguments.

```dream
public class GenSymbolInstance
```

## `public ty: GenTypeRef`

```dream
public ty: GenTypeRef
```

## `public bases: List<GenTypeRef>`

```dream
public bases: List<GenTypeRef>
```

## `public fields: List<GenField>`

```dream
public fields: List<GenField>
```

## `public methods: List<GenFunction>`

```dream
public methods: List<GenFunction>
```

## `public variants: List<GenVariant>`

```dream
public variants: List<GenVariant>
```

## `public constructor(borrow v: JsonValue)`

Decodes an instantiated type view exported by semantic analysis.

```dream
public constructor(borrow v: JsonValue)
```
