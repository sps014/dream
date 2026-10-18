---
title: Dream
summary: A statically typed language that compiles to fast native binaries and WebAssembly.
template: home.html
hide: [navigation, toc, path, footer]
---

<div class="dream-hero" markdown>

<div class="dream-hero__text" markdown>

<p class="dream-hero__eyebrow">Native · WebAssembly · Node</p>

# Fast programs, everywhere { .dream-hero__title }

<p class="dream-hero__tagline">A typed language that compiles to native binaries and WebAssembly.</p>

<div class="dream-hero__actions" markdown>

[Install Dream](learn/quickstart.md){ .md-button .md-button--primary }
[Take the tour](learn/tour.md){ .md-button }

</div>

</div>

<div class="dream-hero__code" markdown>

```dream
import system;
import system.collections;

fun main() {
    let scores: Map<string, int> = {
        "Ada": 92, "Sam": 78
    };

    for (let entry in scores) {
        System.println($"{entry.key}: {entry.value}");
    }
}
```

</div>

</div>

## Explore the docs { .dream-home__heading }

<!-- pages -->

## Why Dream { .dream-home__heading }

<div class="dream-why" markdown>

<div markdown>

### Native and WebAssembly

One codebase builds a native executable, a browser module, or a Node module.

</div>

<div markdown>

### ARC that compiles away

Last-use moves, inferred borrows, and retain elision remove most reference counting at build time.

</div>

<div markdown>

### Seamless interop

Call C and C++ with `@c` and `@cpp`, and JavaScript with `extern` and the `js` type.

</div>

<div markdown>

### Typed web APIs

Compile-time routing, validation, authentication, OpenAPI, and WebSockets in the standard library.

</div>

<div markdown>

### Zero-cost generics

Generic code is specialized for each type you use, with no runtime dispatch.

</div>

<div markdown>

### Async and parallel

`async`/`await` for waiting work, `Task` for parallel CPU work, both owned by structured scopes.

</div>

</div>

## GPU programming in Dream { .dream-home__heading }

Write compute, vertex, and fragment shaders as Dream functions with `import system.gpu`.
Shaders compile ahead of time, and generated calls connect them to your application with typed
parameters and GPU buffers. Use them for parallel computation, drawing into textures, or rendering
in a window.

Install the shader compiler with `dreamer toolchain install slang`, then follow the
[GPU setup and first compute shader](library/gpu/index.md). WebGPU supports native and browser
programs; Metal, Vulkan, Direct3D, and CUDA provide additional native targets. CUDA is compute-only.
Backend and hardware support determine which advanced features you can use, and GPU operations
report failures through `Result` values with `GpuError` details.

- [Rendering](library/gpu/rendering.md): draw with vertex and fragment shaders.
- [Interactive spotlight](cookbook/gpu-spotlight.md): draw a light that follows the mouse.
- [Ray tracing](library/gpu/ray-tracing.md): trace rays on supported native backends and hardware.
- [Machine learning](library/gpu/machine-learning.md): differentiate shader code and use tensor hardware.
