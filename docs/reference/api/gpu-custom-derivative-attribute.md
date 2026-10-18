# CustomDerivativeAttribute

**Import:** `import system.gpu;`

This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation. Shader-only declarations can be used only inside shader entry points and helpers reached from them; using them elsewhere is a compile error.

## `CustomDerivativeAttribute`

Supplies a hand-written backward derivative for a `@differentiable` helper, replacing the automatic one. Use it when the automatic derivative is slow or numerically unstable.  `function` names a inferred shader helper whose parameters are the original parameters as `ref` `GpuDiffPair` values, followed by the derivative of the result. It writes each parameter's derivative into its pair.

```dream
public struct CustomDerivativeAttribute
```

## `function`

The helper that computes the backward derivative.

```dream
public function: DeclarationPath
```
