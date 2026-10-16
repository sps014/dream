# GenCompilationError

**Import:** `import system.codegen;`

## `GenCompilationError`

A diagnostic associated with a declaration in the generator's input.

```dream
public class GenCompilationError
```

## `target`

Target declaration or build target associated with this result.

```dream
public target: string
```

## `message`

Human-readable explanation of the result or failure.

```dream
public message: string
```

## `constructor`

Decodes a pre-generation error restricted to this generator's triggered declarations.

```dream
public constructor(borrow v: JsonValue)
```
