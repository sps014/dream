# GenModule

**Import:** `import system.codegen;`

## `GenModule`

A user module whose `module` declaration carries one of the generator's trigger attributes. Module attributes hold settings that apply to every declaration in the module; combine them with per-declaration attributes through `GenContext.module_of`.

```dream
public class GenModule
```

## `id`

Stable identifier of the module (`module:<path>`). Pass it to `GenContext.error_at` to report a diagnostic at the module declaration.

```dream
public id: string
```

## `name`

Dotted module path, for example `shaders.lighting`.

```dream
public name: string
```

## `file`

Project-relative path of the file whose `module` declaration carries the first trigger attribute.

```dream
public file: string
```

## `location`

Location of that `module` declaration.

```dream
public location: GenLocation
```

## `attributes`

Attributes on the module's declarations, in file order. When several files declare the same module, their attributes are combined.

```dream
public attributes: List<GenAttribute>
```

## `constructor`

Creates the snapshot model from the supplied values.

```dream
public constructor(
    id: string,
    name: string,
    file: string,
    location: GenLocation,
    attributes: List<GenAttribute>
)
```

## `attribute`

Decodes the first attribute of type `A` on the module, or `None` when it has none.

```dream
public fun attribute<A>(): Option<A>
```

## `attributes_of`

Decodes every attribute of type `A` on the module, in source order.

```dream
public fun attributes_of<A>(): List<A>
```

## `has`

Returns whether the module carries an attribute of type `A`.

```dream
public fun has<A>(): bool
```
