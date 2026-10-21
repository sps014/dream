# GenIndexEntry

**Import:** `import system.codegen;`

## `GenIndexEntry`

Every type in the program (user and stdlib), names and kinds only.

```dream
public class GenIndexEntry
```

## `id`

`module::name` identity of the type (`system.collections::List`).

```dream
public id: string
```

## `name`

Unqualified type name.

```dream
public name: string
```

## `kind`

Whether the type is a class, struct, union, enum, or interface.

```dream
public kind: DeclKind
```

## `constructor`

Creates a `GenIndexEntry` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(id: string, name: string, kind: DeclKind)
```
