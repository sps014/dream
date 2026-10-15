# GenAttributes

**Import:** `import system.codegen;`

## `public static class GenAttributes`

Typed attribute access. `A` is an `@attribute` struct; the compiler gives every such type the statics these forward to.

```dream
public static class GenAttributes
```

## `id`

Returns the compiler identity of an attribute type.

```dream
public static extern fun id<A>(): string
```

## `decode`

Decodes the first matching attribute value.

```dream
public static extern fun decode<A>(borrow attrs: List<GenAttribute>): Option<A>
```

## `decode_all`

Decodes every matching attribute value in source order.

```dream
public static extern fun decode_all<A>(borrow attrs: List<GenAttribute>): List<A>
```

## `has`

Tests whether an attribute with the given compiler identity is present.

```dream
public static fun has(borrow attrs: List<GenAttribute>, borrow id: string): bool
```
