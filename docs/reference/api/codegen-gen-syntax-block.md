# GenSyntaxBlock

**Import:** `import system.codegen;`

## `GenSyntaxBlock`

A `name { ... }` site of this generator (`@syntax_block` generators only).

```dream
public class GenSyntaxBlock
```

## `id`

Identity of this model node; pass it to `GenContext.error_at` to report a diagnostic here.

```dream
public id: string
```

## `name`

Block name as written before `{` (`html` in `html { ... }`).

```dream
public name: string
```

## `body`

Reconstructed body text; splices appear as `{expr}`.

```dream
public body: string
```

## `splices`

Dream source of each `{ ... }` splice, in order.

```dream
public splices: List<string>
```

## `location`

Where the block is written in source.

```dream
public location: GenLocation
```

## `constructor`

Creates a `GenSyntaxBlock` from its parts. Generators normally receive these from `GenContext` rather than constructing them.

```dream
public constructor(
    id: string,
    name: string,
    body: string,
    splices: List<string>,
    location: GenLocation
)
```
