# GenSnapshot

**Import:** `import system.codegen;`

Read the [usage guide](../stdlib/codegen.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `class GenSnapshot`

Decoded snapshot behind a `GenContext`.

```dream
public class GenSnapshot
```

## `version: int`

```dream
public version: int
```

## `generator: string`

```dream
public generator: string
```

## `generator_id: string`

```dream
public generator_id: string
```

## `target: string`

```dream
public target: string
```

## `options: JsonValue`

```dream
public options: JsonValue
```

## `files: List<GenFile>`

```dream
public files: List<GenFile>
```

## `index: List<GenIndexEntry>`

```dream
public index: List<GenIndexEntry>
```

## `decls: List<GenDecl>`

```dream
public decls: List<GenDecl>
```

## `functions: List<GenFunction>`

```dream
public functions: List<GenFunction>
```

## `blocks: List<GenSyntaxBlock>`

```dream
public blocks: List<GenSyntaxBlock>
```

## `calls: List<GenCallSite>`

```dream
public calls: List<GenCallSite>
```

## `constructor`

```dream
public constructor(root: JsonValue)
```
