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

Stable identifier of this entry.

```dream
public id: string
```

## `kind`

Category of this entry.

```dream
public kind: string
```

## `declaration`

Declaration associated with this entry.

```dream
public declaration: Option<GenDecl>
```

## `function`

Function declaration associated with this entry, when available.

```dream
public function: Option<GenFunction>
```

## `constructor`

Decodes a declaration or function symbol from the compilation snapshot.

```dream
public constructor(borrow v: JsonValue)
```
