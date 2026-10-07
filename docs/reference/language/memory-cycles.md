# Cycles and deterministic ARC

Recursive classes and strong reference cycles are supported. You can build lists, trees,
parent/child graphs, callback graphs, and cyclic containers without mandatory ownership
annotations or a tracing collector.

The compiler emits exact strong-edge metadata for concrete heap layouts. Types that cannot
participate in cycles use ordinary ARC. Cycle-capable objects use localized trial deletion:
internal references are subtracted from scratch counts, and objects still reachable from
external owners survive. An unreachable remainder is reclaimed synchronously before the
outermost ownership-release boundary returns. Reentrant cleanup joins the same drain.

## Destructors and observers

For a cyclic group, weak handles become empty and unowned handles become invalid before
user destructors run. Destructors run once in allocation order while peer storage and strong
fields remain readable. After all finalizers finish, edges are cleared and storage reclaimed.
Finalizers may temporarily read dying peers. They may not resurrect them, publish them into
an escaping owner, or mutate their strong edges.

Debug and Release share these semantics. Debug also performs ownership validation and leak
reporting. Explicit `defer` postpones destruction intentionally and drains pending cycles
before final leak reporting.

## Weak and unowned references

`weak` and `unowned` remain useful when a reference should not extend a lifetime. A weak field
has type `Option<T>` for a class `T` and becomes `None` when the target dies. An unowned field
has a class type and traps if accessed after invalidation. These tools can reduce ownership
traffic and make application intent clearer; they are not required to permit recursive types.

Infinitely sized inline value types remain invalid. A recursive class is a heap reference and
does not have this size problem. Interfaces, erased `object` values and closures are classified
conservatively because their concrete target varies at runtime.

## Concurrency

Cycle-capable reference operations and collection are initially serialized by a runtime gate.
User destructors execute outside that gate. Applications must still synchronize field access.
Cleanup is deterministic for the same synchronized execution; thread scheduling does not gain
a new global ordering guarantee.

## Graph ownership

Release can infer a `UniqueRegion` for proven nonescaping, destructor-free graphs. It excludes
collector-managed objects until equivalent cleanup is proven. A future explicit graph-owner
API could represent an intentionally shared lifetime, but there is no new graph syntax or
lifetime system in this design.
