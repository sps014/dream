# WindowEvent

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `SafeAreaChanged`

The distances to avoid at the drawable edges changed. Window.safe_area holds the latest value after poll; platforms without reported insets start with zero on every edge.

```dream
SafeAreaChanged(insets: WindowInsets)
```
