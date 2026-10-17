# KeyCode

**Import:** `import system.window;`

Read the [usage guide](../../library/system/window.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

This type spans several pages: [part 1](window-key-code-1.md) · [part 2](window-key-code-2.md) · [part 3](window-key-code-3.md) · [part 4](window-key-code-4.md) · [part 5](window-key-code-5.md).

## `KeyCode`

A physical key, named after its position on a US keyboard.  Key codes do not change with the keyboard layout, so `KeyCode.W`, `KeyCode.A`, `KeyCode.S` and `KeyCode.D` are the same four keys on QWERTY, AZERTY and Dvorak keyboards. Use `KeyEvent.key` when you need the character the active layout prints for a key, and `WindowEvent.Text` for typed text.

```dream
public enum KeyCode
```

## `Unknown`

A key the platform reported without a known physical position.

```dream
Unknown = 0
```

## `A`

The key labelled A on a US keyboard.

```dream
A = 1
```

## `B`

The key labelled B on a US keyboard.

```dream
B = 2
```

## `C`

The key labelled C on a US keyboard.

```dream
C = 3
```

## `D`

The key labelled D on a US keyboard.

```dream
D = 4
```

## `E`

The key labelled E on a US keyboard.

```dream
E = 5
```

## `F`

The key labelled F on a US keyboard.

```dream
F = 6
```

## `G`

The key labelled G on a US keyboard.

```dream
G = 7
```

## `H`

The key labelled H on a US keyboard.

```dream
H = 8
```

## `I`

The key labelled I on a US keyboard.

```dream
I = 9
```

## `J`

The key labelled J on a US keyboard.

```dream
J = 10
```

## `K`

The key labelled K on a US keyboard.

```dream
K = 11
```

## `L`

The key labelled L on a US keyboard.

```dream
L = 12
```

## `M`

The key labelled M on a US keyboard.

```dream
M = 13
```

## `N`

The key labelled N on a US keyboard.

```dream
N = 14
```

## `O`

The key labelled O on a US keyboard.

```dream
O = 15
```

## `P`

The key labelled P on a US keyboard.

```dream
P = 16
```

## `Q`

The key labelled Q on a US keyboard.

```dream
Q = 17
```

## `R`

The key labelled R on a US keyboard.

```dream
R = 18
```

## `S`

The key labelled S on a US keyboard.

```dream
S = 19
```

## `T`

The key labelled T on a US keyboard.

```dream
T = 20
```

## `U`

The key labelled U on a US keyboard.

```dream
U = 21
```

## `V`

The key labelled V on a US keyboard.

```dream
V = 22
```
