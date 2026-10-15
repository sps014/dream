# GenSymbol

**Import:** `import system.codegen;`

## `public class GenSymbol`

Signature views allow program-wide queries without including unrelated function bodies.

```dream
public class GenSymbol
```

## `public instances: List<GenSymbolInstance>`

```dream
public instances: List<GenSymbolInstance>
```

## `public id: string`

```dream
public id: string
```

## `public kind: string`

```dream
public kind: string
```

## `public declaration: Option<GenDecl>`

```dream
public declaration: Option<GenDecl>
```

## `public function: Option<GenFunction>`

```dream
public function: Option<GenFunction>
```

## `public constructor(borrow v: JsonValue)`

Decodes a declaration or function symbol from the compilation snapshot.

```dream
public constructor(borrow v: JsonValue)
```
