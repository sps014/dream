# GpuId3

**Import:** `import system.gpu;`

Read the [usage guide](../stdlib/gpu.md) for examples and common tasks. This reference lists the public declarations for this part of the library. See [Reading API signatures](index.md#reading-a-signature) for parameter and result notation.

## `struct GpuId3`

Three-component integer id used by `@compute` builtins (`global_id`, `local_id`, `workgroup_id`, `num_workgroups`). Each field is a workgroup / thread coordinate.

```dream
public struct GpuId3
```

## `x: int`

X index.

```dream
public x: int
```

## `y: int`

Y index.

```dream
public y: int
```

## `z: int`

Z index.

```dream
public z: int
```
