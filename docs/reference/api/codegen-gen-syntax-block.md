# GenSyntaxBlock

**Import:** `import system.codegen;`

## `GenSyntaxBlock`

A `name { ... }` site of this generator (`@syntax_block` generators only).

```dream
public class GenSyntaxBlock
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

Source location associated with this entry.

```dream
public location: GenLocation
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(
    id: string,
    name: string,
    body: string,
    splices: List<string>,
    location: GenLocation
)
```
