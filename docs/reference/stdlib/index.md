# Standard library

The standard library ships with the compiler. Import each package you need — there is no `import system.*;`. Bootstrap types (`Option`, `Result`, `Math`, …) need no import.

## Core

| Page | Import |
| --- | --- |
| [Built-ins](builtins.md) | `import system;` (console, Math, Time) |
| [Option & Result](option-result.md) | none (bootstrap) |
| [Lock & Semaphore](sync.md) | `import system;` / sync types |

## Text and data

| Page | Import |
| --- | --- |
| [Strings](string.md) | `import system.text;` (also via `import system;`) |
| [Regex](regex.md) | `import system.text;` |
| [SIMD](simd.md) | `import system.simd;` |
| [Encoding](encoding.md) | `import system.encoding;` |
| [JSON](json.md) | `import system.json;` |

## Collections

[List, Map, Set, Queue, Stack](collections.md) — `import system.collections;`

## System

| Page | Import |
| --- | --- |
| [Random](random.md) | `import system;` |
| [DateTime](datetime.md) | `import system;` |
| [Logging](logging.md) | `import system.logging;` |
| [Testing](testing.md) | `import system.testing;` |
| [Process](process.md) | `import system;` |
| [WebView](webview.md) | `import system.webview;` |
| [Crypto](crypto.md) | `import system.crypto;` |

## I/O

| Page | Import |
| --- | --- |
| [Files](file.md) | `import system.io;` |
| [HTTP](http.md) | `import system.net;` |
| [Web API](webapi.md) | `import system.webapi;` |
| [Raw sockets](net.md) | `import system.net;` |

## GPU

[system.gpu](gpu.md) — `import system.gpu;` (also see [compute shaders](../language/compute.md) and [vertex & fragment](../language/shaders.md))

## Metaprogramming

[CodeBuilder](codegen.md) — `import system.codegen;` (see [Source generators](../language/generators.md))
