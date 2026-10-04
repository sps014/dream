# GpuPointer

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuPointer`

Latched pointer state for a `GpuSurface` (pixels in surface/client space).

```dream
public struct GpuPointer
```

## `x: float`

Cursor X in surface/client pixels.

```dream
public x: float
```

## `y: float`

Cursor Y in surface/client pixels.

```dream
public y: float
```

## `dx: float`

Delta since the previous `pointer()` read (cleared by the host on each read).

```dream
public dx: float
```

## `dy: float`

Delta since the previous `pointer()` read (cleared by the host on each read).

```dream
public dy: float
```

## `buttons: int`

Bit0 = primary, bit1 = secondary, bit2 = middle.

```dream
public buttons: int
```

## `down: bool`

True while any tracked button is held.

```dream
public down: bool
```

## `inside: bool`

True while the pointer is inside the surface client area.

```dream
public inside: bool
```

## `pointer_id: int`

Primary tracked pointer id, or `-1` if none.

```dream
public pointer_id: int
```
