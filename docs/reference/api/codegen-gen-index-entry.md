# GenIndexEntry

**Import:** `import system.codegen;`

## `GenIndexEntry`

Every type in the program (user and stdlib), names and kinds only.

```dream
public class GenIndexEntry
```

## `id`

Stable identifier of this entry.

```dream
public id: string
```

## `name`

Name identifying this entry.

```dream
public name: string
```

## `kind`

Category of this entry.

```dream
public kind: DeclKind
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, kind: DeclKind)
```
