# Iterator

No import is needed.

Read the [usage guide](../stdlib/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `interface Iterator<T>`

Pull one element at a time. Concrete iterators (`ListIterator`, …) implement this. `@next` lives on the implementing class (attributes are not required on the interface).

```dream
public interface Iterator<T>
```
