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
