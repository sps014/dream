# GenSyntaxBlock

**Import:** `import system.codegen;`

## `public class GenSyntaxBlock`

A `name { ... }` site of this generator (`@syntax_block` generators only).

```dream
public class GenSyntaxBlock
```

## `public id: string`

```dream
public id: string
```

## `public name: string`

```dream
public name: string
```

## `public body: string`

Reconstructed body text; splices appear as `{expr}`.

```dream
public body: string
```

## `public splices: List<string>`

Dream source of each `{ ... }` splice, in order.

```dream
public splices: List<string>
```

## `public location: GenLocation`

```dream
public location: GenLocation
```

## `public constructor(id: string, name: string, body: string, splices: List<string>, location: GenLocation)`

Creates the snapshot model from the supplied values.

```dream
public constructor(id: string, name: string, body: string, splices: List<string>, location: GenLocation)
```
