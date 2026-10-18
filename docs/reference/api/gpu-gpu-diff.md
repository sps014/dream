# GpuDiff

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `GpuDiff`

Automatic differentiation of `@differentiable` helpers inside shaders.  Pass the helper itself as `f`, followed by one `GpuDiffPair` per parameter; the name's number is the helper's parameter count (`forward` and `backward` for one parameter). `forward` returns the result and its derivative along the pairs' derivatives. `backward` takes the pairs by `ref`, plus the derivative of whatever the result feeds into (`1` to differentiate the result itself), and stores each parameter's gradient in its pair's `derivative`. Parameters marked `@no_diff` take plain values instead of pairs.

```dream
public static class GpuDiff
```

## `forward`

Forward-mode derivative of a one-parameter helper.

```dream
public static extern fun forward<A, R>(f: fun(A): R, a: GpuDiffPair<A>): GpuDiffPair<R>
```

## `forward2`

Forward-mode derivative of a two-parameter helper.

```dream
public static extern fun forward2<A, B, R>(
    f: fun(A, B): R,
    a: GpuDiffPair<A>,
    b: GpuDiffPair<B>
): GpuDiffPair<R>
```

## `forward3`

Forward-mode derivative of a three-parameter helper.

```dream
public static extern fun forward3<A, B, C, R>(
    f: fun(A, B, C): R,
    a: GpuDiffPair<A>,
    b: GpuDiffPair<B>,
    c: GpuDiffPair<C>
): GpuDiffPair<R>
```

## `backward`

Reverse-mode gradient of a one-parameter helper.

```dream
public static extern fun backward<A, R>(f: fun(A): R, ref a: GpuDiffPair<A>, d_output: R): void
```

## `backward2`

Reverse-mode gradient of a two-parameter helper.

```dream
public static extern fun backward2<A, B, R>(
    f: fun(A, B): R,
    ref a: GpuDiffPair<A>,
    ref b: GpuDiffPair<B>,
    d_output: R
): void
```

## `backward3`

Reverse-mode gradient of a three-parameter helper.

```dream
public static extern fun backward3<A, B, C, R>(
    f: fun(A, B, C): R,
    ref a: GpuDiffPair<A>,
    ref b: GpuDiffPair<B>,
    ref c: GpuDiffPair<C>,
    d_output: R
): void
```
