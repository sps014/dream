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
