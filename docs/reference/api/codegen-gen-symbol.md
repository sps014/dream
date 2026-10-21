# GenSymbol

**Import:** `import system.codegen;`

## `GenSymbol`

Signature views allow program-wide queries without including unrelated function bodies.

```dream
public class GenSymbol
```

## `instances`

Concrete uses of this generic symbol.

```dream
public instances: List<GenSymbolInstance>
```

## `id`

`module::name` identity of the symbol.

```dream
public id: string
```

## `kind`

Whether the symbol is a type, function, or other kind of declaration.

```dream
public kind: GenSymbolKind
```

## `declaration`

The type declaration this symbol names, when it names one.

```dream
public declaration: Option<GenDecl>
```

## `function`

The function declaration this symbol names, when it names one.

```dream
public function: Option<GenFunction>
```

## `constructor`

Decodes a declaration or function symbol from the compilation snapshot.

```dream
public constructor(borrow v: JsonValue)
```
