# GpuMods

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuMods`

Keyboard modifier latches for a `GpuSurface`.

```dream
public struct GpuMods
```

## `shift: bool`

Shift key held.

```dream
public shift: bool
```

## `ctrl: bool`

Control key held.

```dream
public ctrl: bool
```

## `alt: bool`

Alt / Option key held.

```dream
public alt: bool
```

## `meta: bool`

Cmd on macOS / Windows key on PC.

```dream
public meta: bool
```
