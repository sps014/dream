# GeneratorOnlyAttribute

**Import:** `import system.codegen;`

## `GeneratorOnlyAttribute`

Declares a name that exists only in a generator's target language, such as a shader built-in. The compiler type-checks uses of it like any other declaration, so generators see fully typed facts, but it is never compiled: code may use it only inside a function that carries one of the listed attributes and that a generator consumes. Any other use is an error.  Apply it to bodiless `extern` functions (which may then be `public` and generic, including function-typed and `ref` parameters) and to classes, structs and interfaces, whose members are covered too.

```dream
public struct GeneratorOnlyAttribute
```

## `attribute`

An `@attribute` type; functions carrying it may use the declaration.

```dream
public attribute: DeclarationPath
```

## `additional_attributes`

More `@attribute` types that also grant use.

```dream
public additional_attributes: List<DeclarationPath>
```
