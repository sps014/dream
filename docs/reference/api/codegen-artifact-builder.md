# ArtifactBuilder

**Import:** `import system.codegen;`

## `public class ArtifactBuilder`

```dream
public class ArtifactBuilder
```

## `public source_map: GenSourceMap`

```dream
public source_map: GenSourceMap
```

## `public constructor()`

Creates an empty builder.

```dream
public constructor()
```

## `append`

Appends text and advances the source-map position.

```dream
public fun append(text: string): void
```

## `mapped`

Appends text mapped to the given snapshot node identity.

```dream
public fun mapped(text: string, target: string): void
```

## `line`

Appends a complete line of artifact text.

```dream
public fun line(text: string): void
```

## `to_string`

Returns the accumulated artifact text.

```dream
public override fun to_string(): string
```
