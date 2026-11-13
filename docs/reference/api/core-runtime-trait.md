# RuntimeTrait

No import is needed.

Read the [usage guide](../../library/core/builtins.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `RuntimeTrait`

Behavior of a host service bound with `@runtime`.

```dream
public enum RuntimeTrait
```

## `Component`

The standalone WASI component adapter implements the service. A component build rejects every other host import while it can still report a diagnostic.

```dream
Component
```

## `Quiet`

The service reads only host state: it never touches Dream storage, retains or releases Dream values, or calls back into the program, so the optimizer may move ownership operations across the call. It does not make the call pure or let the optimizer hoist it.

```dream
Quiet
```
