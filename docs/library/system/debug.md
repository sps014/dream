---
title: Debug
summary: Inspect memory use while developing.
icon: material/bug-outline
---

# Check memory use while developing

`Debug` helps you inspect memory use. Import `system` to use it. These values are development aids; they do not coordinate work running in different tasks.

Only debug builds count allocations. In a `--release` build, `live_objects`, `total_allocations` and `free_list_head` always read `0`, so the same code compiles in both modes but measures nothing in release. Run memory checks in a debug build.

| Member | Type | Meaning |
| --- | --- | --- |
| `live_objects` | `long` | Number of live allocated objects, excluding permanent shared values |
| `total_allocations` | `long` | Total allocation count, including reused storage |
| `ref_count(value)` | `int` | Number of strong references to a managed value |
| `heap_ptr` | `int` | A memory-position probe; its meaning depends on the environment |
| `free_list_head` | `int` | A memory-reuse probe, not a portable address |

## Compare before and after

The following is a snippet for a function that already defines `churn()`:

```dream
let before: long = Debug.live_objects;
churn();
let delta: long = Debug.live_objects - before;
System.println(delta);
```

Finish worker tasks before comparing a final count. During concurrent work, different counters may reflect different instants. A changed count alone does not prove a leak: values may still be alive intentionally.

Desktop debug programs print a leak report to stderr at exit:

```text
[dream] leak check: live=2 total_allocations=41 live_bytes=96
[dream] leak by type: Node=2 (96 B)
```

`live` counts objects still allocated, `live_bytes` is the memory they occupy, and the second line lists the 16 largest types by bytes, followed by an `other=` summary when more types leaked. Release programs contain no leak tracking and print no report. Embedded libraries leave reporting to their host application.

See [Debug declarations](../../reference/api/debug.md) and [Memory management](../../language/memory/index.md).
