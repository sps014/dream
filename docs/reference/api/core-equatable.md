# Equatable

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `interface Equatable<T>`

Value equality independent of reference identity.

```dream
public interface Equatable<T>
```

## `equals`

```dream
fun equals(borrow other: T): bool
```
