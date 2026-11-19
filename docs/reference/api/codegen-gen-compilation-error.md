# GenCompilationError

**Import:** `import system.codegen;`

## `GenCompilationError`

A diagnostic associated with a declaration in the generator's input.

```dream
public class GenCompilationError: Error
```

## `target`

Target declaration or build target associated with this result.

```dream
public target: string
```

## `code`

Returns `EGENERATE`, the code shared by every generator input diagnostic.

```dream
public fun code(): string
```

## `message`

Returns the compiler's explanation of the failure in `target`.

```dream
public fun message(): string
```

## `to_string`

Returns `message()`, so printing the error shows its explanation.

```dream
public override fun to_string(): string
```
