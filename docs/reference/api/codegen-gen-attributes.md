# GenAttributes

**Import:** `import system.codegen;`

## `GenAttributes`

Typed access to custom attributes declared with `@attribute`.

```dream
public static class GenAttributes
```

## `id`

Returns the stable identifier of attribute type `A`.

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

Returns whether an attribute with identifier `id` is present.

```dream
public static fun has(borrow attrs: List<GenAttribute>, borrow id: string): bool
```
